# Export a Obsidian con merge y enriquecimiento con Claude

Fecha: 2026-10-12. Esta spec cubre el primer subproyecto de tres. Las tarjetas Anki y la integración con Calibre van a tener su propia spec.

## Objetivo

Hoy `highlights_export` (`src-tauri/src/lib.rs`) reescribe cada nota con `std::fs::write`, así que borra lo que el usuario agrega a mano. El nuevo export encuentra las bóvedas de Obsidian, agrega solo los subrayados nuevos a las notas que ya existen y puede llamar a `claude -p` para sumar un resumen, etiquetas y enlaces a otras notas de la bóveda.

Criterios de éxito:

1. Si exportás dos veces sin cambios en la Kindle, el archivo no cambia.
2. Si agregás texto a mano en una nota y exportás de nuevo, el texto sigue ahí y los subrayados nuevos quedan al final.
3. "Enriquecer con Claude" edita solo la nota del libro, con el modelo que elegiste (Haiku, Sonnet, Opus o Fable; por defecto Sonnet).

## Datos verificados

- Obsidian guarda sus bóvedas en `~/Library/Application Support/obsidian/obsidian.json`, en la clave `vaults.<id>.path`. En esta Mac hay una sola bóveda: `~/Documents/Obsidian Vault`, con obsidian-git.
- `claude` 2.1.296 es un binario Mach-O en `/opt/homebrew/bin/claude`. Los alias `haiku`, `sonnet`, `opus` y `fable` responden con `claude-haiku-5-5`, `claude-sonnet-5-5`, `claude-opus-5-5` y `claude-fable-5-1`.
- `--tools Read,Glob,Grep,Edit --strict-mcp-config --setting-sources project` deja a Claude con esas 4 herramientas y sin servidores MCP. Una edición simple con Haiku tardó 5,8 s.
- `--bare` no sirve, porque desactiva el login OAuth de Claude Code.

## Diseño

### Bóvedas

El comando `obsidian_vaults` lee `obsidian.json` y devuelve las rutas que existen. El selector de la vista Subrayados muestra esas bóvedas. Al elegir una, `vault_dir` pasa a ser `<bóveda>/KOReader`. "Otra carpeta…" abre el diálogo de carpetas como hoy. Si no hay ninguna bóveda, el botón "Crear bóveda" crea `~/Documents/KOReader Vault/.obsidian/` y la abre con `obsidian://open?path=...` para que Obsidian la registre.

### Formato de la nota

```
---
title: "Libro"
author: "Autor"
progress: 42%
highlights: 12
last_read: 2026-10-11
read_time: 5 h 10 min
source: koreader
tags:
  - koreader
---

# Libro
*Autor*

## Capítulo 1

> texto del subrayado
> — Capítulo 1 · p. 5 · 2026-08-05
> **Nota:** texto de la nota

^kob-1a2b3c4d
```

- El ID `^kob-xxxxxxxx` es un ID de bloque de Obsidian. Sale de un hash FNV-1a de 64 bits de `datetime` y `text`, en 8 dígitos hex. FNV es estable entre versiones de Rust, y `DefaultHasher` no lo es.
- La nota del usuario queda dentro de la cita, así un enlace al bloque la incluye.
- El ID va en la línea siguiente a la cita, sin línea en blanco, como en el ejemplo de la ayuda de Obsidian (obsidian.md/help/links).
- Los subrayados sin capítulo van bajo `## Subrayados`, así siempre hay un encabezado antes de las citas.
- `last_read` y `read_time` salen de la copia local de `statistics.sqlite3` (tabla `book`, emparejada por título). Si no hay copia o no hay fila, la clave no se escribe.

### Merge

1. Si la nota no existe, la app escribe la nota completa.
2. Si existe y tiene IDs `^kob-`, la app agrega al final solo los subrayados cuyo ID no está. Antes de cada subrayado agrega `## <capítulo>` cuando el capítulo cambia respecto del último encabezado del archivo. Después actualiza en el frontmatter solo `progress`, `highlights`, `last_read` y `read_time`.
3. Si existe y no tiene ningún ID (export de la 0.1.1), la app la renombra a `<título> (anterior).md` y escribe una nota nueva.
4. Nunca se borran subrayados de la nota, aunque se hayan borrado en la Kindle.

`Libros.md` en la misma carpeta es la nota índice. La app la reescribe entera en cada export, con una tabla de libro, autor, progreso, subrayados y última lectura.

### Enriquecer con Claude

El comando `enrich_start(books, model, events)` exporta esos libros y después procesa una nota por vez:

```
claude -p <prompt> --model <alias> --output-format stream-json --verbose
  --tools Read,Glob,Grep,Edit --permission-mode acceptEdits
  --strict-mcp-config --setting-sources project --no-session-persistence
  --disable-slash-commands
```

- La carpeta de trabajo es la raíz de la bóveda, que es el primer ancestro de `vault_dir` con `.obsidian/`, o `vault_dir` si no hay ninguno.
- El modelo solo puede ser `haiku`, `sonnet`, `opus` o `fable`. El backend rechaza cualquier otro valor. El default es `sonnet` y se guarda en `Settings.ai_model`.
- El prompt pide `## Resumen` con 3 a 5 viñetas basadas solo en las citas, de 3 a 6 etiquetas nuevas en `tags:` y `## Relacionado` con enlaces `[[...]]` solo a notas que existen. Las dos secciones van entre la línea del autor y el primer capítulo, así los subrayados que agrega el merge al final quedan debajo de los capítulos. El prompt también le prohíbe tocar citas e IDs y editar otros archivos.
- Como la bóveda está dentro de `~`, Claude Code también lee `~/CLAUDE.md`. Solo suma contexto al prompt y no cambia los permisos.
- Si la bóveda no tiene `.git`, la app copia la nota a `<nota>.md.kob.bak` antes de llamar a Claude.
- Cuando Claude termina bien, la app escribe `enriched: <fecha>` en el frontmatter.
- La app busca `claude` en `/opt/homebrew/bin`, `/usr/local/bin` y `~/.local/bin`, y si no lo encuentra prueba `zsh -lc 'command -v claude'`.
- Eventos al frontend por `Channel`: `start`, `tool` ("Leyendo Libros.md"), `done` y `error`, cada uno con el título del libro.
- `enrich_stop` mata el proceso y corta la cola.

### Interfaz

- En la lista de libros, cada fila tiene una casilla para seleccionar. El encabezado del artículo tiene "Enriquecer con Claude" para el libro abierto. Si hay libros seleccionados, aparece "Enriquecer N libros".
- El selector de modelo es un control segmentado de 4 opciones con nombre y una línea de detalle: Haiku "más rápido", Sonnet "equilibrado", Opus "más profundo", Fable "el más nuevo". La elección queda guardada.
- Mientras corre, un panel muestra el avance por libro y el botón Cancelar.

## Errores

- Si falta `claude`, el error dice "No encontré Claude Code. Instalalo con `brew install --cask claude-code`".
- Si `claude` termina con `is_error` o con código distinto de 0, el libro queda marcado con error y la cola sigue con el siguiente.
- Si escribir una nota falla, el export se corta y muestra la ruta.

## Pruebas

- Unitarias en `obsidian.rs`: ID estable, nota nueva, merge que agrega solo lo nuevo, merge idempotente, respeto del texto manual, nota legacy detectada, actualización de frontmatter, parseo de `obsidian.json` y parseo de eventos `stream-json`.
- Prueba `#[ignore]` que corre `claude -p` con Haiku sobre una bóveda temporal y verifica que aparece `## Resumen` y que las citas siguen iguales.

## Fuera de esta spec

Tarjetas Anki, la vista Biblioteca y Calibre.
