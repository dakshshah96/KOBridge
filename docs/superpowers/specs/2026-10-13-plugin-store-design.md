# Store de plugins, patches y fuentes

Fecha: 2026-10-13. Esta spec es el primero de cuatro subproyectos: Store, Restaurar respaldos, Limpieza de la Kindle y Captura de pantalla. Cada uno lleva su propia spec.

## Objetivo

Una vista nueva, "Store", muestra el catálogo de Storefront y permite instalar, actualizar y desinstalar plugins, patches y fuentes en la Kindle por SSH. Antes de cada cambio, la app hace un respaldo completo.

Criterios de éxito:

1. Instalar un plugin desde KOBridge deja la Kindle igual que si lo hubiera instalado Storefront: la carpeta en `koreader/plugins/` y la entrada en `Storefront_installs.lua`. Storefront lo muestra como instalado.
2. Cada instalación, actualización o desinstalación deja un `.tar` nuevo en la carpeta de respaldos antes de tocar la Kindle.
3. Si se corta el Wi-Fi o SSH en cualquier paso, la versión anterior del plugin sigue funcionando.
4. La vista abre el catálogo en menos de 200 ms con la copia en caché.

## Datos verificados

- El catálogo está en `https://ultimatejimmy.github.io/storefront.koplugin/catalog.json`. Pesa 1.045.422 bytes, se regenera cada hora y tiene las claves `version`, `generated_at`, `stats`, `plugins` (694), `patches` (149 repos) y `fonts` (21). Lo bajé el 2026-10-13.
- Campos de cada entrada del catálogo:
  - Plugin: `full_name`, `owner`, `name`, `description`, `default_branch`, `stars`, `pushed_at`, `updated_at`, `repo_id`, `latest_release` y `latest_prerelease`. Los dos últimos traen `tag_name`, `download_url`, `published_at` y `assets[]`. 672 de los 694 plugins tienen `latest_release`.
  - Patch: los mismos campos de repo, más `patch_files[]` con `filename`, `path`, `branch`, `sha`, `size` y `download_url`.
  - Fuente: `name`, `full_name`, `font_family`, `font_file`, `category`, `license` y `download_url`, que apunta a un zip en el sitio github.io.
- Storefront (repo ultimatejimmy/storefront.koplugin, GPL-3.0) instala así:
  - Plugins: usa el zip del release si existe. Si no, usa `https://github.com/<owner>/<repo>/archive/refs/heads/<branch>.zip`.
  - Patches: copia el `.lua` a `koreader/patches/` (`storefront_patch_mgr.lua:23`).
  - Fuentes: descomprime el zip en `koreader/fonts/<font_name>/` (`storefront_font_mgr.lua:350` y `:817`).
- El registro de Storefront es `koreader/settings/Storefront_installs.lua`, escrito con `LuaSettings` (`storefront_installs.lua:9`). Tiene una sola clave, `["installs"]`, cuyo valor es un string JSON con `plugins`, `patches`, `fonts` e `item_options`. Storefront lo guarda en memoria y lo escribe con `settings:flush()`, así que KOReader abierto pisa cualquier cambio externo.
- Campos de cada registro:
  - Plugin: `dirname`, `meta_path`, `installed_version`, `installed_tag`, `matched_at`, `plugin_name`, `repo_full_name`, `repo_id`, `repo_description`, `owner`, `branch` y `repo`.
  - Patch: `filename`, `repo_id`, `repo`, `owner`, `sha`, `download_url`, `repo_full_name`, `branch`, `path`, `matched_at` y `repo_description`.
  - Fuente: `is_storefront`, `font_name`, `full_name`, `download_url`, `version`, `installed_at` y `full_installed`.
- La Kindle tiene Storefront en el canal beta, con 9 plugins, 3 patches y 1 fuente registrados. En el JSON, los plugins usan `dirname` como clave, los patches el nombre de archivo instalado (que puede terminar en `.disabled`) y las fuentes `font_name` en minúsculas. Storefront escribe una tabla vacía como `[]`.
- `aria2c` 1.37.0 está en `/opt/homebrew/bin/aria2c` (GPL-2.0-or-later). Depende de openssl@4, sqlite, libssh2, c-ares, gettext y libxml2 de Homebrew, así que no sirve copiarlo dentro de la app.
- KOBridge ya tiene lo que este flujo necesita:
  - `lua::parse` (`src-tauri/src/lua.rs`) lee las tablas que escribe KOReader.
  - `reader_running()` (`lib.rs:579`) sabe si KOReader está abierto.
  - `backup_run` (`lib.rs:144`) hace el respaldo completo y reporta los bytes por un `Channel<u64>`.

## Diseño

### Componentes

- `src-tauri/src/aria.rs`: encuentra `aria2c`, arma sus argumentos, lo corre y lee su avance. No sabe nada de plugins.
- `src-tauri/src/store.rs`: catálogo, registro, instalación y desinstalación. Usa `aria.rs`, `ssh.rs` y `lua.rs`.
- `src/lib/views/Store.svelte`: la vista, con tres pestañas.
- Comandos de Tauri en `lib.rs`:
  - `store_catalog(refresh)`
  - `store_installed()`
  - `store_install(kind, id, progress)`
  - `store_uninstall(kind, id, progress)`

  `kind` es `plugin`, `patch` o `font`. El `id` es `full_name` en plugins, `full_name` más `path` en patches, y `name` en fuentes.

### aria2c

La app busca `aria2c` en el `PATH` del shell de login (`$SHELL -lc 'command -v aria2c'`, igual que `find_claude`). Si no lo encuentra, usa la copia embebida como sidecar de Tauri (`bundle.externalBin`).

Argumentos de cada descarga:

```
aria2c --no-conf=true
  -x 8 -s 8 -k 1M
  --file-allocation=none
  --continue=true --max-tries=5 --retry-wait=2
  --connect-timeout=10 --timeout=30
  --auto-file-renaming=false --allow-overwrite=true
  --http-accept-gzip=true
  --summary-interval=1 --show-console-readout=false --enable-color=false
  --download-result=hide --console-log-level=warn
  -U KOBridge/<versión>
  -d <dir> -o <archivo> <url>
```

- Con `-k 1M`, aria2 parte un archivo en varias conexiones solo si pesa más de 2 MiB. Los zips del catálogo van de 10 KB a 6 MB.
- El catálogo se baja a `catalog.new.json` y reemplaza a `catalog.json` solo si se puede parsear, así una descarga cortada no rompe la copia que usa el modo sin internet.
- En Linux se suma `--ca-certificate=/etc/ssl/certs/ca-certificates.crt` si el archivo existe.
- La app lee las líneas `[#<gid> <bajado>/<total>(<pct>%) CN:<n> DL:<velocidad>]` de stdout y manda los bytes por el `Channel`.

Copia embebida:

- macOS (arm64): un job de CI compila aria2 1.37.0 desde el tarball oficial con `--with-appletls --without-openssl --without-gnutls --without-libxml2 --without-libexpat --without-sqlite3 --without-libssh2 --without-libcares`, y revisa con `otool -L` que solo enlace librerías del sistema.
- Linux: el `.deb` y el `.AppImage` llevan el mismo `aria2c`, compilado por el workflow de Linux con `scripts/build-aria2.sh`. Enlaza glibc, OpenSSL 3 y zlib en forma dinámica, porque una glibc estática no resuelve nombres en otras distribuciones, y la app ya depende de esas mismas librerías.
- Licencia: aria2 es GPL-2.0-or-later y KOBridge es PolyForm Noncommercial. La app lleva aria2c como ejecutable aparte y sin modificar, junto con `licenses/aria2-COPYING` y un enlace al tag 1.37.0 del código fuente en el README.

### Catálogo

- `store_catalog` baja el catálogo a `app_cache_dir()/catalog.json` y lo devuelve reducido, con los campos que muestra la vista. Si la descarga falla y hay copia, devuelve la copia con su `generated_at`, así la vista indica de cuándo es.
- La vista llama a `store_catalog(false)` al abrir y usa la caché si tiene menos de 1 hora. El botón Actualizar llama a `store_catalog(true)`.

### Instalados

`store_installed` corre un solo `ssh` que devuelve:

- `koreader/settings/Storefront_installs.lua`
- La lista de `koreader/plugins/*.koplugin` con el `version` de cada `_meta.lua`
- La lista de `koreader/patches/*.lua`
- La lista de carpetas de `koreader/fonts/`

Hay actualización disponible en estos casos:

- Plugin con release: el `tag_name` del catálogo es distinto de `installed_tag`.
- Plugin sin release: `pushed_at` del catálogo es posterior a `matched_at`.
- Patch: el `sha` del catálogo es distinto del registrado.

Las fuentes no tienen actualización, porque el catálogo no publica versión ni fecha por fuente. Se desinstalan y se vuelven a instalar.

Un plugin que está en `koreader/plugins/` pero no en el registro aparece como instalado, sin opción de actualizar. Puede venir con KOReader o haberse instalado a mano.

### Instalar o actualizar

1. Si `reader_running()` da verdadero, la app corta y pide cerrar KOReader.
2. Corre el respaldo completo, sumando la carpeta del plugin o de la fuente que se va a reemplazar, y reporta los bytes.
3. Baja el archivo con aria2c a una carpeta temporal nueva dentro de `app_cache_dir()/store/`.
4. Para un plugin, descomprime el zip con la crate `zip` y busca la carpeta `*.koplugin` que tiene `_meta.lua` y `main.lua`, igual que Storefront. Rechaza entradas con `..` o rutas absolutas.
5. Manda la carpeta con `tar c | ssh 'tar x -C <destino>.kob-new'`, como el envío de libros.
6. En un solo comando remoto: `mv <destino> <destino>.kob-old`, `mv <destino>.kob-new <destino>` y `rm -rf <destino>.kob-old`. Si el segundo `mv` falla, el mismo comando devuelve `.kob-old` a su nombre.
7. Actualiza el registro (ver abajo).
8. La vista muestra "Listo. Reiniciá KOReader para cargarlo."

Los patches siguen los mismos pasos con un solo archivo, sin descomprimir. Las fuentes se descomprimen en `koreader/fonts/<font_name>/`.

### Desinstalar

Revisa que KOReader esté cerrado, hace el respaldo completo, borra la carpeta o el archivo con `rm -rf` y quita la entrada del registro. La lista de rutas que la app puede borrar suma `koreader/plugins/<x>.koplugin`, `koreader/patches/<x>.lua` y `koreader/fonts/<x>`, validando que `<x>` no tenga `/` ni `..`. Los plugins que no están en el registro no se pueden desinstalar desde la app.

### Registro

- La app lee `Storefront_installs.lua` con `lua::parse`, toma el string de `installs` y lo parsea con `serde_json` como `serde_json::Value`. Así conserva los campos que no conoce.
- Cambia solo la entrada del ítem instalado o borrado, y llena los campos con los mismos nombres que usa Storefront. `matched_at` e `installed_at` usan `os.time()` en segundos, igual que Storefront.
- Escribe el archivo como `return {\n    ["installs"] = "<json escapado>",\n}\n`, con el mismo escape de strings que usa `LuaSettings`. Lo sube como `Storefront_installs.lua.kob-new` y lo renombra con `mv`.
- Si el archivo no existe, la app lo crea con `plugins`, `patches`, `fonts` e `item_options` vacíos.

### Interfaz

- La vista Store va en la barra lateral, debajo de Patches.
- Tiene tres pestañas, Plugins, Patches y Fuentes, con la búsqueda arriba. La búsqueda filtra por nombre, dueño y descripción mientras escribís.
- Se puede ordenar por estrellas o por última actualización, y filtrar para ver solo los instalados.
- Cada fila muestra el nombre, el dueño, las estrellas, la descripción en una línea y un botón: Instalar, Actualizar o Desinstalar.
- La fila elegida cambia solo de fondo.
- Mientras corre una operación, la fila muestra el paso (Respaldando, Bajando, Enviando) con una barra, y los botones de las demás filas quedan deshabilitados.
- Si KOReader está abierto, un aviso arriba de la lista explica que hay que cerrarlo, y los botones quedan deshabilitados.
- Los textos usan `t(en, es)`, como el resto de la app.

## Errores

| Falla | Qué hace la app |
|---|---|
| KOReader abierto | No hace nada y pide cerrarlo |
| Falla el respaldo | Corta antes de bajar nada |
| Falla la descarga | Corta sin tocar la Kindle y muestra el error de aria2c |
| El zip no trae `_meta.lua` y `main.lua` | Corta sin tocar la Kindle y dice qué archivo falta |
| Se corta SSH durante el envío | La carpeta anterior queda intacta. El siguiente intento borra `<destino>.kob-new` antes de empezar |
| Falla el segundo `mv` | Devuelve `<destino>.kob-old` a su nombre y muestra el error |
| El registro no se puede parsear | Instala igual, no toca el registro y avisa que Storefront no lo va a mostrar como instalado |
| No hay internet y no hay catálogo en caché | La vista dice que necesita internet la primera vez |

En todos los casos queda el respaldo completo de ese momento, que la feature Restaurar respaldos va a poder usar.

## Pruebas

- Pruebas de Rust, sin Kindle:
  - `store.rs` lee una copia del `Storefront_installs.lua` de la Kindle (7 plugins, 3 patches, 1 fuente), lo reescribe sin cambios y el resultado es igual al original.
  - Agregar y quitar un plugin cambia solo esa entrada.
  - El zip se elige bien: con release usa el del release, sin release usa el de la rama.
  - Encuentra la carpeta `*.koplugin` en un zip con carpeta raíz y en uno sin ella, y rechaza un zip con `../`.
  - `aria.rs` lee las líneas de avance de aria2 y arma los argumentos con y sin `--ca-certificate`.
- Pruebas en vivo, marcadas `#[ignore]`: instalar y desinstalar un plugin chico del catálogo en la Kindle, y revisar después en Storefront que aparezca como instalado.
- `bun run check` da 0 errores y 0 warnings.
- CI compila aria2c en macOS y en Linux, y `otool -L` en macOS no muestra rutas de `/opt/homebrew`.

## Fuera de esta spec

- Restaurar un respaldo o una sola carpeta de plugin: es la spec de Restaurar respaldos.
- Activar o desactivar plugins con `plugins_disabled`.
- Búsqueda directa en la API de GitHub, fuera del catálogo.
- Screensavers y blueprints de Storefront.
- Builds de aria2c para macOS Intel y Linux arm64.
