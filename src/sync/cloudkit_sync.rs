//! CloudKit sync transport — pushes/pulls CRR changesets via Apple CloudKit.
//!
//! Uses the private CloudKit database with a custom zone "RoteroSync".
//! Each changeset batch is stored as a CKRecord of type "Changeset".

use rotero_db::crr::{self, ChangeRow};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{AllocAnyThread, ClassType, msg_send, msg_send_id};
use objc2_cloud_kit::*;
use objc2_foundation::*;

use std::cell::Cell;
use std::ptr::NonNull;

const CONTAINER_ID: &str = "iCloud.com.rotero.Rotero";
const ZONE_NAME: &str = "RoteroSync";
const RECORD_TYPE: &str = "Changeset";

pub struct CloudKitSyncEngine {
    site_id: Vec<u8>,
    zone_id: Retained<CKRecordZoneID>,
    database: Retained<CKDatabase>,
    zone_created: bool,
}

impl CloudKitSyncEngine {
    pub fn new(site_id: Vec<u8>) -> Result<Self, String> {
        // SAFETY: CloudKit ObjC API calls are safe when:
        // 1. NSString::from_str creates valid autoreleased strings
        // 2. CKContainer/CKDatabase are thread-safe CloudKit objects
        // 3. CKRecordZoneID alloc+init follows standard ObjC ownership
        unsafe {
            let container_id = NSString::from_str(CONTAINER_ID);
            let container = CKContainer::containerWithIdentifier(&container_id);
            let database = container.privateCloudDatabase();

            let zone_name = NSString::from_str(ZONE_NAME);
            let zone_id = CKRecordZoneID::initWithZoneName_ownerName(
                CKRecordZoneID::alloc(),
                &zone_name,
                CKCurrentUserDefaultName,
            );

            Ok(Self {
                site_id,
                zone_id,
                database,
                zone_created: false,
            })
        }
    }

    fn site_id_hex(&self) -> String {
        self.site_id.iter().map(|b| format!("{b:02x}")).collect()
    }

    async fn ensure_zone(&mut self) -> Result<(), String> {
        if self.zone_created {
            return Ok(());
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Cell::new(Some(tx));

        // SAFETY: ObjC CloudKit zone creation — CKRecordZone and CKModifyRecordZonesOperation
        // are safe to construct; completion block captures are Send-safe via Cell<Option<Sender>>.
        unsafe {
            let zone = CKRecordZone::initWithZoneID(CKRecordZone::alloc(), &self.zone_id);
            let zones = NSArray::from_retained_slice(&[zone]);

            let op = CKModifyRecordZonesOperation::initWithRecordZonesToSave_recordZoneIDsToDelete(
                CKModifyRecordZonesOperation::alloc(),
                Some(&zones),
                None,
            );

            let block = block2::RcBlock::new(
                move |_saved: *mut NSArray<CKRecordZone>,
                      _deleted: *mut NSArray<CKRecordZoneID>,
                      error: *mut NSError| {
                    if let Some(tx) = tx.take() {
                        if error.is_null() {
                            let _ = tx.send(Ok(()));
                        } else {
                            let desc = (*error).localizedDescription().to_string();
                            let _ = tx.send(Err(desc));
                        }
                    }
                },
            );
            op.setModifyRecordZonesCompletionBlock(Some(&block));
            self.database.addOperation(&op);
        }

        rx.await
            .map_err(|_| "Zone creation channel closed".to_string())??;
        self.zone_created = true;
        Ok(())
    }

    /// Returns count of changes pushed.
    pub async fn export_changes(
        &mut self,
        conn: &rotero_db::turso::Connection,
    ) -> Result<usize, String> {
        let last_ver = read_i64_state(conn, "cloudkit_last_exported_ver").await;

        let changes = crr::changes_since(conn, last_ver)
            .await
            .map_err(|e| format!("Failed to read changes: {e}"))?;

        if changes.is_empty() {
            return Ok(0);
        }

        self.ensure_zone().await?;

        let current_ver = crr::current_db_version(conn)
            .await
            .map_err(|e| format!("Failed to read db_version: {e}"))?;

        let payload = serde_json::to_vec(&changes).map_err(|e| format!("Serialize failed: {e}"))?;

        let record_name = format!("{}_{:08}_{:08}", self.site_id_hex(), last_ver, current_ver,);

        // SAFETY: ObjC CloudKit record creation and save operation — standard CKRecord
        // construction with NSData/NSString values. CKModifyRecordsOperation is thread-safe.
        unsafe {
            let record_id = CKRecordID::initWithRecordName_zoneID(
                CKRecordID::alloc(),
                &NSString::from_str(&record_name),
                &self.zone_id,
            );
            let record_type = NSString::from_str(RECORD_TYPE);
            let record =
                CKRecord::initWithRecordType_recordID(CKRecord::alloc(), &record_type, &record_id);

            let payload_data = NSData::with_bytes(&payload);
            let payload_val: &ProtocolObject<dyn CKRecordValue> =
                ProtocolObject::from_ref(&*payload_data);
            record.setObject_forKey(Some(payload_val), &NSString::from_str("payload"));

            let site_data = NSData::with_bytes(&self.site_id);
            let site_val: &ProtocolObject<dyn CKRecordValue> =
                ProtocolObject::from_ref(&*site_data);
            record.setObject_forKey(Some(site_val), &NSString::from_str("siteId"));

            let from_num: Retained<NSNumber> =
                msg_send_id![NSNumber::class(), numberWithLongLong: last_ver];
            let from_val: &ProtocolObject<dyn CKRecordValue> = ProtocolObject::from_ref(&*from_num);
            record.setObject_forKey(Some(from_val), &NSString::from_str("fromVersion"));

            let to_num: Retained<NSNumber> =
                msg_send_id![NSNumber::class(), numberWithLongLong: current_ver];
            let to_val: &ProtocolObject<dyn CKRecordValue> = ProtocolObject::from_ref(&*to_num);
            record.setObject_forKey(Some(to_val), &NSString::from_str("toVersion"));

            self.save_records(&[record]).await?;
        }

        let count = changes.len();
        write_i64_state(conn, "cloudkit_last_exported_ver", current_ver).await;
        Ok(count)
    }

    /// Returns count of changes applied.
    pub async fn import_changes(
        &mut self,
        conn: &rotero_db::turso::Connection,
    ) -> Result<usize, String> {
        self.ensure_zone().await?;

        let token_bytes = crr::get_sync_state(conn, "cloudkit_server_token").await;
        let token = token_bytes.as_deref().and_then(deserialize_server_token);

        let (records, new_token) = self.fetch_zone_changes(token.as_deref()).await?;

        let my_hex = self.site_id_hex();
        let mut total_applied = 0;

        for record in &records {
            // SAFETY: Reading CKRecord fields via ObjC message send — objectForKey returns
            // autoreleased NSData/NSString. Retained::retain on non-null pointers is valid.
            unsafe {
                let site_obj = record.objectForKey(&NSString::from_str("siteId"));
                let Some(site_obj) = site_obj else { continue };
                let site_data: *const NSData = msg_send![&*site_obj, self];
                if site_data.is_null() {
                    continue;
                }
                let site_bytes = (*site_data).to_vec();
                let site_hex: String = site_bytes.iter().map(|b| format!("{b:02x}")).collect();

                if site_hex == my_hex {
                    continue;
                }

                let payload_obj = record.objectForKey(&NSString::from_str("payload"));
                let Some(payload_obj) = payload_obj else {
                    continue;
                };
                let payload_data: *const NSData = msg_send![&*payload_obj, self];
                if payload_data.is_null() {
                    continue;
                }
                let payload_bytes = (*payload_data).to_vec();

                let changes: Vec<ChangeRow> = match serde_json::from_slice(&payload_bytes) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("Failed to deserialize CloudKit changeset: {e}");
                        continue;
                    }
                };

                match crr::apply_changes(conn, &changes).await {
                    Ok(result) => total_applied += result.applied,
                    Err(e) => tracing::warn!("Failed to apply CloudKit changes: {e}"),
                }
            }
        }

        if let Some(ref token) = new_token {
            if let Some(bytes) = serialize_server_token(token) {
                let _ = crr::set_sync_state(conn, "cloudkit_server_token", &bytes).await;
            }
        }

        Ok(total_applied)
    }

    async fn save_records(&self, records: &[Retained<CKRecord>]) -> Result<(), String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Cell::new(Some(tx));

        // SAFETY: CKModifyRecordsOperation with batch save — ObjC operation is thread-safe.
        // Completion block captures Cell<Option<Sender>> which is consumed exactly once.
        unsafe {
            let records_array = NSArray::from_retained_slice(records);
            let op = CKModifyRecordsOperation::initWithRecordsToSave_recordIDsToDelete(
                CKModifyRecordsOperation::alloc(),
                Some(&records_array),
                None,
            );

            let block = block2::RcBlock::new(
                move |_saved: *mut NSArray<CKRecord>,
                      _deleted: *mut NSArray<CKRecordID>,
                      error: *mut NSError| {
                    if let Some(tx) = tx.take() {
                        if error.is_null() {
                            let _ = tx.send(Ok(()));
                        } else {
                            let desc = (*error).localizedDescription().to_string();
                            let _ = tx.send(Err(desc));
                        }
                    }
                },
            );
            op.setModifyRecordsCompletionBlock(Some(&block));
            self.database.addOperation(&op);
        }

        rx.await
            .map_err(|_| "Save records channel closed".to_string())?
    }

    async fn fetch_zone_changes(
        &self,
        token: Option<&CKServerChangeToken>,
    ) -> Result<
        (
            Vec<Retained<CKRecord>>,
            Option<Retained<CKServerChangeToken>>,
        ),
        String,
    > {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let tx = Cell::new(Some(tx));
        let records = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let records_clone = records.clone();

        // SAFETY: CKFetchRecordZoneChangesOperation — ObjC fetch operation is thread-safe.
        // Callback blocks capture Arc<Mutex<Vec>> and Cell<Option<Sender>> safely.
        unsafe {
            let config = CKFetchRecordZoneChangesConfiguration::init(
                CKFetchRecordZoneChangesConfiguration::alloc(),
            );
            if let Some(t) = token {
                config.setPreviousServerChangeToken(Some(t));
            }

            let zone_ids = NSArray::from_retained_slice(&[self.zone_id.clone()]);

            let keys: Vec<&CKRecordZoneID> = vec![&*self.zone_id];
            let values: Vec<&CKFetchRecordZoneChangesConfiguration> = vec![&*config];
            let config_dict = NSDictionary::from_slices(&keys, &values);

            let op = CKFetchRecordZoneChangesOperation::initWithRecordZoneIDs_configurationsByRecordZoneID(
                CKFetchRecordZoneChangesOperation::alloc(),
                &zone_ids,
                Some(&config_dict),
            );

            let records_for_cb = records_clone.clone();
            let changed_block = block2::RcBlock::new(move |record: NonNull<CKRecord>| {
                let Some(retained) = Retained::retain(record.as_ptr()) else {
                    return;
                };
                records_for_cb
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(retained);
            });
            #[allow(deprecated)]
            op.setRecordChangedBlock(Some(&changed_block));

            let completion_block = block2::RcBlock::new(
                move |_zone_id: NonNull<CKRecordZoneID>,
                      token: *mut CKServerChangeToken,
                      _data: *mut NSData,
                      _more: Bool,
                      error: *mut NSError| {
                    if let Some(tx) = tx.take() {
                        if error.is_null() {
                            let new_token = if token.is_null() {
                                None
                            } else {
                                Retained::retain(token)
                            };
                            let _ = tx.send(Ok(new_token));
                        } else {
                            let desc = (*error).localizedDescription().to_string();
                            let _ = tx.send(Err(desc));
                        }
                    }
                },
            );
            op.setRecordZoneFetchCompletionBlock(Some(&completion_block));
            self.database.addOperation(&op);
        }

        let new_token = rx
            .await
            .map_err(|_| "Fetch changes channel closed".to_string())??;
        let fetched_records = records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect();
        Ok((fetched_records, new_token))
    }
}

async fn read_i64_state(conn: &rotero_db::turso::Connection, key: &str) -> i64 {
    crr::get_sync_state(conn, key)
        .await
        .and_then(|bytes| {
            if bytes.len() >= 8 {
                bytes
                    .get(..8)
                    .and_then(|b| b.try_into().ok())
                    .map(i64::from_le_bytes)
            } else {
                None
            }
        })
        .unwrap_or(0)
}

async fn write_i64_state(conn: &rotero_db::turso::Connection, key: &str, value: i64) {
    let _ = crr::set_sync_state(conn, key, &value.to_le_bytes()).await;
}

/// Serialize a CKServerChangeToken to bytes via NSKeyedArchiver.
fn serialize_server_token(token: &CKServerChangeToken) -> Option<Vec<u8>> {
    // SAFETY: NSKeyedArchiver serialization of NSCoding-conforming CKServerChangeToken.
    // archivedDataWithRootObject returns autoreleased NSData or nil (handled by Option).
    unsafe {
        let data: Option<Retained<NSData>> = msg_send_id![
            NSKeyedArchiver::class(),
            archivedDataWithRootObject: token,
            requiringSecureCoding: true,
            error: std::ptr::null_mut::<*mut NSError>()
        ];
        data.map(|d| d.to_vec())
    }
}

/// Deserialize a CKServerChangeToken from bytes via NSKeyedUnarchiver.
fn deserialize_server_token(bytes: &[u8]) -> Option<Retained<CKServerChangeToken>> {
    // SAFETY: NSKeyedUnarchiver deserialization — NSData::with_bytes creates valid NSData,
    // unarchivedObjectOfClass returns nil (Option::None) on failure.
    unsafe {
        let data = NSData::with_bytes(bytes);
        let token: Option<Retained<CKServerChangeToken>> = msg_send_id![
            NSKeyedUnarchiver::class(),
            unarchivedObjectOfClass: CKServerChangeToken::class(),
            fromData: &*data,
            error: std::ptr::null_mut::<*mut NSError>()
        ];
        token
    }
}
