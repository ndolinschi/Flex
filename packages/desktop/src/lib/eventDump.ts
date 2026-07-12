// Debug-only raw session-event capture.
//
// This used to own its own ring buffer + `localStorage["flex.debugEvents"]`
// flag; it's now a thin adapter over `lib/debug/log.ts` so there's ONE debug
// switch (Settings toggle / `localStorage["flex.debug"]`) and ONE export
// covering both the leveled log and this raw-event firehose. Kept as a
// separate module (rather than inlining into `useGlobalSessionEvents.ts`)
// so that hook's existing imports keep working unchanged. The
// `window.__flexEventDump`/`__flexDumpSave` legacy globals are still wired
// up — see `lib/debug/log.ts`.
export {
  isEventDumpEnabled,
  recordRawEvent,
  exportDebugLog as flexDumpSave,
} from "./debug/log"
