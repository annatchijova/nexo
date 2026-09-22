/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_NEXO_API_BASE?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
