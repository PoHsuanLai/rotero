export const manifest = (() => {
function __memo(fn) {
	let value;
	return () => value ??= (value = fn());
}

return {
	appDir: "_app",
	appPath: "_app",
	assets: new Set([]),
	mimeTypes: {},
	_: {
		client: {start:"_app/immutable/entry/start.DVf6Q1ST.js",app:"_app/immutable/entry/app.HbbxugJz.js",imports:["_app/immutable/entry/start.DVf6Q1ST.js","_app/immutable/chunks/ruwM9ipk.js","_app/immutable/chunks/B1Aqya1z.js","_app/immutable/entry/app.HbbxugJz.js","_app/immutable/chunks/B1Aqya1z.js","_app/immutable/chunks/DTSuRrcl.js","_app/immutable/chunks/DuJl1Wdv.js","_app/immutable/chunks/DCzVSsBz.js","_app/immutable/chunks/sIFNlO3u.js"],stylesheets:[],fonts:[],uses_env_dynamic_public:false},
		nodes: [
			__memo(() => import('./nodes/0.js')),
			__memo(() => import('./nodes/1.js')),
			__memo(() => import('./nodes/2.js'))
		],
		remotes: {
			
		},
		routes: [
			{
				id: "/",
				pattern: /^\/$/,
				params: [],
				page: { layouts: [0,], errors: [1,], leaf: 2 },
				endpoint: null
			}
		],
		prerendered_routes: new Set([]),
		matchers: async () => {
			
			return {  };
		},
		server_assets: {}
	}
}
})();
