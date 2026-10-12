# KOBridge

KOBridge es una app para macOS que maneja KOReader en una Kindle por SSH, sin cable USB. Con la app podés respaldar la Kindle, ver tus estadísticas de lectura, exportar tus subrayados a Obsidian, mandar libros y fuentes, y editar los patches y la configuración de KOReader.

La app está hecha con Tauri 2, Svelte 5 y un backend en Rust. La interfaz está en español.

## Qué hace cada sección

| Sección | Qué hace |
|---|---|
| Respaldo | Copia a tu Mac los ajustes, las estadísticas, los patches y los subrayados de KOReader en un archivo `.tar`. Puede incluir los libros, y antes de respaldar mide el tamaño real con `du` y la velocidad de la red para estimar cuánto va a tardar. Si lo activás, respalda una vez al día cuando la Kindle aparece en la red. |
| Lectura | Copia `statistics.sqlite3` de KOReader y muestra las horas leídas, los minutos por día del último mes y el progreso de cada libro. |
| Subrayados | Lee los subrayados y las notas de cada libro y escribe un archivo `.md` por libro en una carpeta de Obsidian. |
| Enviar libros | Copia a `/mnt/us/books` los archivos que arrastrás a la ventana. |
| Patches | Lista, edita, activa y desactiva los user patches de `koreader/patches`. |
| Ajustes | Edita `defaults.custom.lua` y `settings.reader.lua`. La app solo guarda `settings.reader.lua` con KOReader cerrado, porque KOReader sobrescribe ese archivo al cerrarse. |
| Registro | Muestra `crash.log` en vivo y puede escribir un mensaje en la pantalla de la Kindle con `fbink`. |
| Fuentes | Sube y borra archivos `.ttf` y `.otf` en `/mnt/us/fonts`. |

La app guarda una copia local de las estadísticas y de los subrayados, así que podés verlos aunque la Kindle esté apagada.

## Requisitos

Necesitás una Kindle con jailbreak y KOReader instalado. Si todavía no lo tenés, seguí la guía de instalación de KOReader para Kindle en su [wiki](https://github.com/koreader/koreader/wiki/Installation-on-Kindle-devices). KOBridge se probó con KOReader v2026.07.1 en una Kindle con firmware 5.18.5.

La app corre en macOS con Apple Silicon. La release solo trae la versión `aarch64`, pero podés compilarla para Intel desde el código.

## Configuración

KOBridge no guarda direcciones IP ni contraseñas. La app usa el comando `ssh` de tu Mac con el nombre `kindle`, así que toda la configuración de la conexión vive en `~/.ssh/config`. Hacé estos pasos una sola vez.

### 1. Activá el servidor SSH en KOReader

Primero, conectá la Kindle al Wi-Fi. Segundo, en KOReader abrí el menú de herramientas (el ícono de engranaje) y entrá a Red, luego a Servidor SSH. Tercero, tocá Iniciar servidor SSH. El servidor escucha en el puerto 2222 y el usuario es `root`.

En el mismo menú, Información de red te muestra la IP de la Kindle. Anotala para el paso 3.

### 2. Creá una llave SSH y pasala a la Kindle

KOReader acepta llaves públicas en el archivo `koreader/settings/SSH/authorized_keys` de la Kindle. Primero, creá una llave solo para la Kindle en tu Mac:

```sh
ssh-keygen -t ed25519 -f ~/.ssh/id_ed25519_kindle -C kindle
```

Después, copiá el contenido de `~/.ssh/id_ed25519_kindle.pub` a `koreader/settings/SSH/authorized_keys` en la Kindle. Lo más simple es conectar la Kindle por USB y pegar la línea en ese archivo con un editor de texto. Si el archivo no existe, crealo.

Otra opción es activar el login sin contraseña en el menú del servidor SSH, entrar una vez y copiar la llave por SSH. Si usás esa opción, apagala apenas termines, porque deja la Kindle abierta para cualquiera en tu red.

### 3. Agregá la Kindle a `~/.ssh/config`

Agregá este bloque a `~/.ssh/config` y cambiá la IP por la de tu Kindle:

```
Host kindle
  HostName 192.168.0.38
  Port 2222
  User root
  IdentityFile ~/.ssh/id_ed25519_kindle
  IdentitiesOnly yes
```

Si la Kindle se conecta a varias redes, cada red le da una IP distinta. En ese caso podés dejar que SSH elija la IP que responde. Reemplazá la línea `HostName` del bloque anterior por estos bloques `Match`, y ponelos antes de `Host kindle`:

```
Match originalhost kindle exec "nc -z -G 1 192.168.0.38 2222 2>/dev/null"
  HostName 192.168.0.38
Match originalhost kindle exec "nc -z -G 1 10.100.200.70 2222 2>/dev/null"
  HostName 10.100.200.70

Host kindle
  HostKeyAlias kindle
  Port 2222
  User root
  IdentityFile ~/.ssh/id_ed25519_kindle
  IdentitiesOnly yes
```

SSH prueba las IPs en orden y usa la primera que tiene el puerto 2222 abierto. La línea `HostKeyAlias kindle` hace que SSH guarde una sola huella de la Kindle para todas sus IPs, así no te pregunta de nuevo cada vez que cambiás de red.

### 4. Probá la conexión

Corré este comando en la Terminal:

```sh
ssh kindle 'cat /mnt/us/koreader/git-rev'
```

La primera vez SSH te pide confirmar la huella de la Kindle, y tenés que responder `yes`. Si el comando imprime la versión de KOReader, la configuración está lista. KOBridge no puede responder esa pregunta por vos, así que hacé esta prueba antes de abrir la app.

### 5. Instalá la app

Primero, bajá el `.dmg` de la [última release](https://github.com/danihrndzld/KOBridge/releases/latest) y arrastrá KOBridge a Aplicaciones.

Segundo, abrí la app. La app no está firmada con un certificado de Apple, así que macOS la bloquea la primera vez. Para abrirla, hacé clic derecho sobre KOBridge en Aplicaciones y elegí Abrir. También podés quitar la marca de cuarentena desde la Terminal:

```sh
xattr -dr com.apple.quarantine /Applications/KOBridge.app
```

Tercero, si macOS te pregunta si KOBridge puede buscar dispositivos en tu red local, respondé que sí. Sin ese permiso, la app no llega a la Kindle aunque `ssh kindle` funcione en la Terminal. Si respondiste que no, podés cambiarlo en Ajustes del Sistema, en Privacidad y seguridad, Red local.

Abajo en la barra lateral, la app muestra si encontró la Kindle, su IP, la batería y el espacio libre. Si no la encuentra, abrí Detalle para ver el error de SSH.

### Carpetas por defecto

La app guarda los respaldos en `~/Backups/kindle` y exporta los subrayados a `~/Documents/Obsidian Vault/KOReader`. Podés cambiar las dos carpetas desde la app, en Respaldo y en Subrayados.

## Límites conocidos

La app no puede reiniciar KOReader. KOReader solo se reinicia cuando lo pide desde adentro, así que después de cambiar un patch o una fuente tenés que reiniciarlo en la Kindle.

El mensaje que escribe `fbink` queda en pantalla hasta que KOReader vuelve a dibujarla.

## Compilar desde el código

Necesitás Rust, [Bun](https://bun.sh) y las Command Line Tools de Xcode.

```sh
bun install
bunx tauri dev            # abre la app en modo desarrollo
bunx tauri build          # crea el .app y el .dmg en src-tauri/target/release/bundle
bun run check             # revisa los tipos de Svelte y TypeScript
```

Las pruebas de Rust están en `src-tauri`:

```sh
cd src-tauri
cargo test                              # pruebas que no necesitan la Kindle
cargo test -- --ignored --nocapture     # pruebas contra una Kindle real
```

Las pruebas con `--ignored` usan tu Kindle por SSH. Crean archivos de prueba en `/tmp` y en `koreader/patches` de la Kindle, y los borran al terminar.
