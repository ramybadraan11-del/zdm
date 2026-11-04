/// <reference types="vite/client" />

/**
 * Vite's client types provide `import.meta.env`. Kept in a dedicated file so
 * editors pick it up without touching tsconfig `types`.
 */
interface ImportMetaEnv {
  readonly DEV: boolean;
  readonly PROD: boolean;
  readonly TAURI_DEBUG?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
