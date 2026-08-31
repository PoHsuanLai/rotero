//! The agent conversation belonging to each subject.
//!
//! A chat is an attribute of what it is about — a paper, a collection, or an
//! ad-hoc set of papers — not an entry in a chronological log. This module owns
//! that mapping so opening a paper can resume its conversation.
//!
//! Local-only, and deliberately so: session ids are minted by the agent binary
//! on this machine, so a row synced to another device would name a session that
//! resolves to nothing there. These tables are therefore absent from
//! [`crate::sync_schema::SYNCED_TABLES`], carry none of the `updated_at` /
//! `updated_by` / `deleted` bookkeeping columns, and have no `_live` view.
//! **Never call [`Database::touch`] for them** — every synced table's write path
//! does, and it would fail here for want of those columns.

use turso::Value;

use crate::Database;

/// What a conversation is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatSubject {
    /// One paper.
    Paper(String),
    /// A collection, by id.
    Collection(String),
    /// An ad-hoc set of papers, identified by its members rather than by a name.
    Group(Vec<String>),
}

impl ChatSubject {
    /// The `subject_kind` column value.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Paper(_) => "paper",
            Self::Collection(_) => "collection",
            Self::Group(_) => "group",
        }
    }

    /// The `subject_id` column value.
    ///
    /// A group has no name to key on, so it is identified by its members: the
    /// ids sorted and joined, which makes the lookup insensitive to the order
    /// they happened to be selected in. Selecting a *different* set yields a
    /// different key, which is correct — it is a different subject.
    pub fn id(&self) -> String {
        match self {
            Self::Paper(id) | Self::Collection(id) => id.clone(),
            Self::Group(ids) => {
                let mut sorted = ids.clone();
                sorted.sort();
                sorted.dedup();
                sorted.join(":")
            }
        }
    }

    /// The papers this subject covers, for the `chat_session_papers` rows.
    /// A collection's membership is resolved by the caller, so it contributes
    /// nothing here on its own.
    pub fn paper_ids(&self) -> Vec<String> {
        match self {
            Self::Paper(id) => vec![id.clone()],
            Self::Collection(_) => Vec::new(),
            Self::Group(ids) => ids.clone(),
        }
    }
}

/// One conversation's index row.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatSessionRow {
    /// The agent's session id — the key used to resume it.
    pub session_id: String,
    pub provider_id: String,
    pub subject_kind: String,
    pub subject_id: Option<String>,
    /// A one-line description, shown where the subject's own name isn't enough.
    pub summary: Option<String>,
    pub created_at: String,
    pub last_used_at: String,
    /// The agent no longer has this session, so a fresh one should replace it.
    pub is_dead: bool,
}

const SELECT_COLS: &str = "session_id, provider_id, subject_kind, subject_id, summary, \
                           created_at, last_used_at, is_dead";

fn row_to_session(row: &turso::Row) -> ChatSessionRow {
    ChatSessionRow {
        session_id: crate::get_text(row, 0),
        provider_id: crate::get_text(row, 1),
        subject_kind: crate::get_text(row, 2),
        subject_id: crate::get_opt_text(row, 3),
        summary: crate::get_opt_text(row, 4),
        created_at: crate::get_text(row, 5),
        last_used_at: crate::get_text(row, 6),
        is_dead: crate::get_bool(row, 7),
    }
}

/// One message in a stored conversation transcript.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatMessageRecord {
    pub id: String,
    pub session_id: String,
    pub seq: i64,
    pub role: String,
    pub content_json: String,
    pub created_at: String,
}

const SELECT_MSG_COLS: &str = "id, session_id, seq, role, content_json, created_at";

fn row_to_message(row: &turso::Row) -> ChatMessageRecord {
    ChatMessageRecord {
        id: crate::get_text(row, 0),
        session_id: crate::get_text(row, 1),
        seq: crate::get_opt_i64(row, 2).unwrap_or(0),
        role: crate::get_text(row, 3),
        content_json: crate::get_text(row, 4),
        created_at: crate::get_text(row, 5),
    }
}

impl Database {
    /// The live conversation for a subject, if one exists.
    ///
    /// Dead sessions are filtered out, so a subject whose conversation the agent
    /// has forgotten reads as having none and a fresh one takes its place.
    pub async fn chat_session_for_subject(
        &self,
        subject: &ChatSubject,
    ) -> Result<Option<ChatSessionRow>, crate::DbError> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {SELECT_COLS} FROM chat_sessions \
             WHERE subject_kind = ?1 AND subject_id = ?2 AND is_dead = 0 \
             ORDER BY last_used_at DESC LIMIT 1"
        );
        let mut rows = conn
            .query(
                &sql,
                [
                    Value::Text(subject.kind().to_string()),
                    Value::Text(subject.id()),
                ],
            )
            .await?;
        Ok(rows.next().await?.map(|row| row_to_session(&row)))
    }

    /// Record a conversation and the papers it covers.
    ///
    /// `created_at` survives a repeat call so the row keeps its original age;
    /// `summary` is only overwritten by a non-null value, so a later upsert that
    /// doesn't know the summary can't erase one already learned.
    pub async fn upsert_chat_session(
        &self,
        row: &ChatSessionRow,
        paper_ids: &[String],
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO chat_sessions \
                 (session_id, provider_id, subject_kind, subject_id, summary, \
                  created_at, last_used_at, is_dead) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(session_id) DO UPDATE SET \
                 provider_id  = excluded.provider_id, \
                 subject_kind = excluded.subject_kind, \
                 subject_id   = excluded.subject_id, \
                 summary      = COALESCE(excluded.summary, chat_sessions.summary), \
                 last_used_at = excluded.last_used_at, \
                 is_dead      = excluded.is_dead",
            turso::params::Params::Positional(vec![
                Value::Text(row.session_id.clone()),
                Value::Text(row.provider_id.clone()),
                Value::Text(row.subject_kind.clone()),
                crate::opt_text(row.subject_id.as_ref()),
                crate::opt_text(row.summary.as_ref()),
                Value::Text(row.created_at.clone()),
                Value::Text(row.last_used_at.clone()),
                Value::Integer(i64::from(row.is_dead)),
            ]),
        )
        .await?;

        // These are the subject's own papers, so they define what the
        // conversation is about.
        for paper_id in paper_ids {
            self.link_chat_session_paper(&row.session_id, paper_id, true)
                .await?;
        }
        Ok(())
    }

    /// Record that a conversation touched a paper. Idempotent.
    ///
    /// `is_subject` separates the papers a conversation is *about* from the ones
    /// it merely read: an agent answering a question runs searches, and every
    /// result would otherwise claim the conversation as its own. A paper already
    /// recorded as a subject stays one — a later incidental mention must not
    /// demote it.
    ///
    /// Unknown ids are dropped rather than inserted: foreign keys are not
    /// enforced, and the agent can name a paper that isn't in the library.
    pub async fn link_chat_session_paper(
        &self,
        session_id: &str,
        paper_id: &str,
        is_subject: bool,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        let mut existing = conn
            .query(
                "SELECT 1 FROM papers_live WHERE id = ?1",
                [Value::Text(paper_id.to_string())],
            )
            .await?;
        if existing.next().await?.is_none() {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO chat_session_papers (session_id, paper_id, is_subject) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(session_id, paper_id) DO UPDATE SET \
                 is_subject = MAX(chat_session_papers.is_subject, excluded.is_subject)",
            [
                Value::Text(session_id.to_string()),
                Value::Text(paper_id.to_string()),
                Value::Integer(i64::from(is_subject)),
            ],
        )
        .await?;
        Ok(())
    }

    /// Attach a one-line description to a conversation.
    pub async fn set_chat_session_summary(
        &self,
        session_id: &str,
        summary: &str,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        // Inserts rather than only updating: the row is created when the agent
        // announces the session, and both writes are spawned independently, so
        // a label written first would update nothing and be lost. The
        // placeholder columns are all overwritten by the upsert that follows.
        conn.execute(
            "INSERT INTO chat_sessions \
                 (session_id, provider_id, subject_kind, subject_id, summary, \
                  created_at, last_used_at, is_dead) \
             VALUES (?1, '', 'general', NULL, ?2, ?3, ?3, 0) \
             ON CONFLICT(session_id) DO UPDATE SET summary = excluded.summary",
            [
                Value::Text(session_id.to_string()),
                Value::Text(summary.to_string()),
                Value::Text(chrono::Utc::now().to_rfc3339()),
            ],
        )
        .await?;
        Ok(())
    }

    /// Note that a conversation was just used, so it sorts as the most recent.
    pub async fn touch_chat_session(
        &self,
        session_id: &str,
        when: &str,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE chat_sessions SET last_used_at = ?1 WHERE session_id = ?2",
            [
                Value::Text(when.to_string()),
                Value::Text(session_id.to_string()),
            ],
        )
        .await?;
        Ok(())
    }

    /// Mark a conversation the agent can no longer load.
    ///
    /// The row is kept rather than deleted so the papers it covered stay on
    /// record; it simply stops being offered for resumption.
    pub async fn mark_chat_session_dead(&self, session_id: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            "UPDATE chat_sessions SET is_dead = 1 WHERE session_id = ?1",
            [Value::Text(session_id.to_string())],
        )
        .await?;
        Ok(())
    }

    /// Every live conversation *about* a paper, most recently used first.
    ///
    /// Deliberately narrower than "touched this paper": an agent answering a
    /// question runs searches, and every result it reads is linked to the
    /// conversation. Listing those would put one chat about one paper on the
    /// panel of every paper it happened to look at. A conversation belongs to a
    /// paper when the paper is its subject, or when it is a member of the group
    /// or collection the conversation is about.
    pub async fn chat_sessions_for_paper(
        &self,
        paper_id: &str,
    ) -> Result<Vec<ChatSessionRow>, crate::DbError> {
        let conn = self.conn();
        // Joined against `papers_live`: a deleted paper is tombstoned rather
        // than removed, so the cascade never fires and the link outlives it.
        // Columns are qualified: `session_id` names a column on both joined
        // tables, so an unqualified list is ambiguous.
        let cols = SELECT_COLS
            .split(", ")
            .map(|c| format!("s.{c}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT {cols} FROM chat_sessions s \
             JOIN chat_session_papers p ON p.session_id = s.session_id \
             JOIN papers_live lp ON lp.id = p.paper_id \
             WHERE p.paper_id = ?1 AND p.is_subject = 1 AND s.is_dead = 0 \
             ORDER BY s.last_used_at DESC"
        );
        let mut rows = conn
            .query(&sql, [Value::Text(paper_id.to_string())])
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row_to_session(&row));
        }
        Ok(out)
    }

    /// Every conversation on record, keyed by session id.
    ///
    /// For joining against a list the agent reports: the agent titles a session
    /// after its first user message, which for these is a synthetic startup
    /// entry, so its titles are uninformative and ours are not.
    pub async fn all_chat_sessions(&self) -> Result<Vec<ChatSessionRow>, crate::DbError> {
        let conn = self.conn();
        let sql = format!("SELECT {SELECT_COLS} FROM chat_sessions");
        let mut rows = conn.query(&sql, ()).await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row_to_session(&row));
        }
        Ok(out)
    }

    /// The subject papers of every conversation, as `(session_id, paper_id)`.
    pub async fn all_chat_session_subjects(&self) -> Result<Vec<(String, String)>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(
                "SELECT p.session_id, p.paper_id FROM chat_session_papers p \
                 JOIN papers_live lp ON lp.id = p.paper_id \
                 WHERE p.is_subject = 1",
                (),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push((crate::get_text(&row, 0), crate::get_text(&row, 1)));
        }
        Ok(out)
    }

    /// The papers a conversation covers, excluding any since deleted.
    pub async fn chat_session_paper_ids(
        &self,
        session_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(
                "SELECT p.paper_id FROM chat_session_papers p \
                 JOIN papers_live lp ON lp.id = p.paper_id \
                 WHERE p.session_id = ?1",
                [Value::Text(session_id.to_string())],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(crate::get_text(&row, 0));
        }
        Ok(out)
    }

    /// Messages in a conversation, in chronological order.
    pub async fn chat_messages_for_session(
        &self,
        session_id: &str,
    ) -> Result<Vec<ChatMessageRecord>, crate::DbError> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {SELECT_MSG_COLS} FROM chat_messages \
             WHERE session_id = ?1 ORDER BY seq ASC"
        );
        let mut rows = conn
            .query(&sql, [Value::Text(session_id.to_string())])
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(row_to_message(&row));
        }
        Ok(out)
    }

    /// Append or update a single message in a conversation.
    pub async fn append_chat_message(
        &self,
        record: &ChatMessageRecord,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO chat_messages (id, session_id, seq, role, content_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(id) DO UPDATE SET \
                 seq = excluded.seq, \
                 role = excluded.role, \
                 content_json = excluded.content_json, \
                 created_at = excluded.created_at",
            turso::params::Params::Positional(vec![
                Value::Text(record.id.clone()),
                Value::Text(record.session_id.clone()),
                Value::Integer(record.seq),
                Value::Text(record.role.clone()),
                Value::Text(record.content_json.clone()),
                Value::Text(record.created_at.clone()),
            ]),
        )
        .await?;
        Ok(())
    }

    /// Save multiple messages for a conversation, preserving order.
    pub async fn save_chat_messages(
        &self,
        session_id: &str,
        records: &[ChatMessageRecord],
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        for rec in records {
            conn.execute(
                "INSERT INTO chat_messages (id, session_id, seq, role, content_json, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(id) DO UPDATE SET \
                     seq = excluded.seq, \
                     role = excluded.role, \
                     content_json = excluded.content_json, \
                     created_at = excluded.created_at",
                turso::params::Params::Positional(vec![
                    Value::Text(rec.id.clone()),
                    Value::Text(session_id.to_string()),
                    Value::Integer(rec.seq),
                    Value::Text(rec.role.clone()),
                    Value::Text(rec.content_json.clone()),
                    Value::Text(rec.created_at.clone()),
                ]),
            )
            .await?;
        }
        Ok(())
    }

    /// Clear all messages for a session.
    pub async fn clear_chat_messages(&self, session_id: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM chat_messages WHERE session_id = ?1",
            [Value::Text(session_id.to_string())],
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        Database::open(dir.path().to_path_buf()).await.unwrap()
    }

    #[tokio::test]
    async fn chat_messages_round_trip() {
        let db = test_db().await;

        let session = ChatSessionRow {
            session_id: "sess-1".into(),
            provider_id: "claude".into(),
            subject_kind: "paper".into(),
            subject_id: Some("p1".into()),
            summary: Some("Discussion on attention mechanisms".into()),
            created_at: "2026-08-31T10:00:00Z".into(),
            last_used_at: "2026-08-31T10:05:00Z".into(),
            is_dead: false,
        };
        db.upsert_chat_session(&session, &[]).await.unwrap();

        let msg1 = ChatMessageRecord {
            id: "msg-1".into(),
            session_id: "sess-1".into(),
            seq: 1,
            role: "user".into(),
            content_json: r#"[{"Text":"What is multi-head attention?"}]"#.into(),
            created_at: "2026-08-31T10:00:05Z".into(),
        };
        let msg2 = ChatMessageRecord {
            id: "msg-2".into(),
            session_id: "sess-1".into(),
            seq: 2,
            role: "assistant".into(),
            content_json: r#"[{"Text":"Multi-head attention allows the model to jointly attend to information..."}]"#.into(),
            created_at: "2026-08-31T10:00:15Z".into(),
        };

        db.append_chat_message(&msg1).await.unwrap();
        db.append_chat_message(&msg2).await.unwrap();

        let loaded = db.chat_messages_for_session("sess-1").await.unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], msg1);
        assert_eq!(loaded[1], msg2);
        assert_eq!(loaded[0].seq, 1);
        assert_eq!(loaded[1].seq, 2);

        // Update an existing message by ID
        let mut msg2_updated = msg2.clone();
        msg2_updated.content_json = r#"[{"Text":"Updated explanation"}]"#.into();
        db.append_chat_message(&msg2_updated).await.unwrap();

        let reloaded = db.chat_messages_for_session("sess-1").await.unwrap();
        assert_eq!(reloaded.len(), 2);
        assert_eq!(
            reloaded[1].content_json,
            r#"[{"Text":"Updated explanation"}]"#
        );

        // Clear messages
        db.clear_chat_messages("sess-1").await.unwrap();
        let cleared = db.chat_messages_for_session("sess-1").await.unwrap();
        assert!(cleared.is_empty());
    }
}
