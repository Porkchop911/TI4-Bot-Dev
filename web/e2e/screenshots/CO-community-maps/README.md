# CO-community-maps

The map picker with the community layouts added in `crates/ti4-server/src/maps/community_maps.json`
(see `crates/ti4-server/docs/COMMUNITY_MAPS.md`), named by their display name.

Rerun: `cd web && npm run screenshots -- CO`

The captures render the real lobby and MapPicker (`_shared/mapLobby.ts`) against routed HTTP, but
the catalog and every pictured board are the real server's answers, snapshotted into `server-snapshot/<n>p.json`
by `node e2e/screenshots/CO-community-maps/fetch-real.mjs` (rerun it after changing the maps).
