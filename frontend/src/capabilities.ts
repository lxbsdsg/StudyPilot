type RuntimeWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
};

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in (window as RuntimeWindow);
}
