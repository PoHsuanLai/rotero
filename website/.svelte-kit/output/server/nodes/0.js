import * as universal from '../entries/pages/_layout.ts.js';

export const index = 0;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/_layout.svelte.js')).default;
export { universal };
export const universal_id = "src/routes/+layout.ts";
export const imports = ["_app/immutable/nodes/0.Dk7Xv2ld.js","_app/immutable/chunks/DuJl1Wdv.js","_app/immutable/chunks/B1Aqya1z.js","_app/immutable/chunks/sIFNlO3u.js","_app/immutable/chunks/CnWK571c.js"];
export const stylesheets = ["_app/immutable/assets/0.D69otjra.css"];
export const fonts = [];
