# Store de plugins: plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Una vista Store que instala, actualiza y desinstala plugins, patches y fuentes del catálogo de Storefront por SSH, con un respaldo completo antes de cada cambio.

**Architecture:** `aria.rs` corre aria2c (el del sistema o el sidecar embebido) y reporta bytes. `store.rs` maneja el catálogo, el registro `Storefront_installs.lua` y las operaciones en la Kindle, reusando `ssh.rs`, `lua.rs` y `backup_run`. `Store.svelte` es la vista.

**Tech Stack:** Rust (Tauri 2, tokio, serde_json, crate `zip`), Svelte 5, aria2 1.37.0.

**Spec:** `docs/superpowers/specs/2026-10-13-plugin-store-design.md`

## Global Constraints

- Commits con `Daniel Hernández <danisnowman@gmail.com>` como autor y committer, sin atribución a Claude.
- Todo texto de la interfaz pasa por `t(en, es)` en Svelte y `tr(en, es)` en Rust.
- `bun run check` con 0 errores y 0 warnings, `cargo test` sin fallas.
- Ninguna operación que escribe en la Kindle corre con KOReader abierto.
- La selección se marca solo con un cambio de fondo y cada acción asíncrona muestra su avance.

## Review Focus

- Un zip de rama trae la carpeta raíz `<repo>-<branch>/` y el plugin adentro como `<x>.koplugin/`, o directamente `_meta.lua` en la raíz. Se esperan ambos casos. Prueba en la tarea 4.
- Un repo cuyo `_meta.lua` está en la raíz del zip no trae nombre de carpeta. Se usa el nombre del repo, con `.koplugin` agregado si le falta. Prueba en la tarea 4.
- Nombres que vienen del catálogo (`font_name`, `filename`, nombre de carpeta) con `/`, `..` o comillas. Se rechazan antes de armar cualquier ruta remota. Prueba en la tarea 5.
- El registro trae `item_options` como `[]` (array vacío de Lua) y `\/` en las URLs. Reescribirlo no puede romperlo para Storefront. Prueba en la tarea 3.
- aria2c no está en el `PATH` del shell de login, y el sidecar tampoco existe (build de desarrollo sin el script). Error claro. Prueba en la tarea 2.

---

### Task 1: aria2c embebido

**Files:**
- Create: `scripts/build-aria2.sh`
- Modify: `src-tauri/tauri.conf.json` (`bundle.externalBin: ["binaries/aria2c"]`), `src-tauri/.gitignore` (`/binaries/`), `.github/workflows/linux.yml`

- [ ] Script que baja aria2 1.37.0, verifica su SHA-256 y compila `src-tauri/binaries/aria2c-<triple>`. En macOS usa AppleTLS sin Homebrew y revisa `otool -L`. En Linux compila estático con OpenSSL.
- [ ] Correrlo en la Mac y revisar `aria2c --version` y `otool -L`.
- [ ] En CI: `sudo apt-get install -y libssl-dev zlib1g-dev pkg-config` y `./scripts/build-aria2.sh` antes de `bun run check`.

### Task 2: `aria.rs`

**Files:** Create `src-tauri/src/aria.rs`

**Interfaces:**
- Produces: `pub fn find() -> Option<PathBuf>`, `pub fn args(url: &str, dir: &Path, out: &str) -> Vec<String>`, `pub fn progress(line: &str) -> Option<(u64, u64)>`, `pub async fn download(url: &str, dir: &Path, out: &str, on: impl FnMut(u64, u64)) -> Res<PathBuf>`

- [ ] Pruebas: `progress("[#2089b0 1.2MiB/5.8MiB(20%) CN:8 DL:2.1MiB]")` da `(1258291, 6081740)`. `args` lleva `--no-conf=true`, `-k` y `1M`, y termina en `-d <dir> -o <archivo> <url>`. `find()` da `None` con `PATH` vacío y sin sidecar.
- [ ] Implementar. `find()` prueba `$SHELL -lc 'command -v aria2c'` y después `current_exe().parent()/aria2c`.

### Task 3: registro de Storefront

**Files:** Create `src-tauri/src/store.rs` (parte registro), fixture `src-tauri/src/fixtures/Storefront_installs.lua` (copia real de la Kindle)

**Interfaces:**
- Produces: `pub fn read_registry(src: &str) -> Res<serde_json::Value>`, `pub fn write_registry(v: &serde_json::Value) -> String`, `pub fn lua_quote(s: &str) -> String`

- [ ] Pruebas: leer el fixture da 9 plugins, 3 patches y 1 fuente. `read_registry(write_registry(v)) == v`. `lua_quote` escapa `\`, `"`, salto de línea y `\0` como `%q`. Un archivo vacío da el objeto con las 4 claves vacías.
- [ ] Implementar con `lua::parse` y `serde_json` con `preserve_order` desactivado (el orden no importa a Storefront).

### Task 4: catálogo y zips

**Files:** `src-tauri/src/store.rs`; agregar crate `zip`

**Interfaces:**
- Produces: `pub fn plugin_zip_url(p: &Value) -> String`, `pub fn find_plugin(dir: &Path, repo: &str) -> Res<(PathBuf, String)>` (carpeta y nombre final `<x>.koplugin`), `pub fn unzip(zip: &Path, dest: &Path) -> Res<()>`, `pub fn slim(catalog: &Value) -> Value`

- [ ] Pruebas: con `latest_release` usa su `download_url`, sin él usa `archive/refs/heads/<branch>.zip`. `find_plugin` encuentra `a-main/x.koplugin/_meta.lua`, `a-main/_meta.lua` (nombre del repo) y rechaza un zip sin `main.lua`. `unzip` rechaza `../x`.
- [ ] Implementar.

### Task 5: operaciones en la Kindle

**Files:** `src-tauri/src/store.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: comandos `store_catalog(refresh: bool) -> Value`, `store_installed() -> Installed`, `store_install(kind, id, progress: Channel<Step>)`, `store_uninstall(kind, id, progress: Channel<Step>)`; `Step { step: &'static str, done: u64, total: u64 }` con `step` en `backup|download|send|done`.

- [ ] Prueba: `safe_name` rechaza `""`, `.`, `..`, nombres con `/` o con caracteres de control.
- [ ] Implementar: KOReader cerrado, `backup_run`, aria2c, `tar` por SSH a `<destino>.kob-new`, intercambio con `mv`, registro.
- [ ] Prueba en vivo `#[ignore]` `store_live`: instala y desinstala `hello` de un repo chico y revisa el registro.

### Task 6: vista Store

**Files:** Create `src/lib/views/Store.svelte`; Modify `src/lib/api.ts`, `src/App.svelte`

- [ ] Tres pestañas, búsqueda, orden por estrellas o fecha, filtro de instalados, botón por fila con su avance, aviso si KOReader está abierto.
- [ ] `bun run check`.

### Task 7: licencia y README

- [ ] `licenses/aria2-COPYING` dentro del bundle (`bundle.resources`), README con la sección del Store y el enlace al código de aria2 1.37.0.
- [ ] Build completo y prueba manual con la Kindle.
