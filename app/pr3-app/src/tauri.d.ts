// The Tauri API that `app.withGlobalTauri` (tauri.conf.json) puts on the window, typed
// by the @tauri-apps/api release that matches the tauri crate.
import type * as core from "@tauri-apps/api/core";
import type * as event from "@tauri-apps/api/event";

declare global {
  interface Window {
    readonly __TAURI__: {
      readonly core: typeof core;
      readonly event: typeof event;
    };
  }
}
