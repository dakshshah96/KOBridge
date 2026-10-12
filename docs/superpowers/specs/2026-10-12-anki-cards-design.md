# Tarjetas Anki desde los subrayados

Fecha: 2026-10-12. Esta spec es el segundo subproyecto de tres. La spec de Obsidian salió antes y la de Calibre viene después.

## Objetivo

El botón "Tarjetas Anki" en Subrayados le pide a `claude -p` preguntas y respuestas sobre los subrayados de uno o más libros. La app las guarda en un archivo que Anki importa.

Criterios de éxito:

1. Cada tarjeta tiene una pregunta que se entiende sola, una respuesta corta y, debajo, el subrayado del que sale, con su capítulo y página.
2. Si importás dos veces el mismo libro, Anki actualiza las tarjetas que ya tenías y no las duplica.
3. Las tarjetas usan el modelo que elegiste en el mismo control que "Enriquecer" (por defecto Sonnet).

## Datos verificados

- Anki 26.9.3 está en `/Applications/Anki.app`, pero nunca se abrió: no existe `~/Library/Application Support/Anki2/` y nada responde en el puerto 8765 de AnkiConnect.
- El importador de texto de Anki lee encabezados `#separator`, `#html`, `#columns`, `#guid column`, `#deck column` y `#tags column` (docs.ankiweb.net/importing/text-files.html). El manual dice que una fila con un GUID que ya existe actualiza esa nota en lugar de crear otra.
- `claude -p --output-format json --json-schema <schema>` devuelve el objeto validado en la clave `structured_output` del evento `result`. Con Haiku, 5 subrayados de "Salvaje de corazón" dieron 8 tarjetas en 12,5 s.

## Diseño

### Por qué un archivo de texto

AnkiConnect necesita Anki abierto y un complemento instalado, y esta Mac no tiene ninguno de los dos. Un `.apkg` necesita armar la base `collection.anki2` y comprimirla en zip. El archivo de texto separado por tabs abre el mismo diálogo de importación y solo necesita escribir líneas.

### Generación

`anki_start(books, model, today, events)` llama a Claude una vez por libro:

```
claude -p <prompt> --model <alias> --output-format json --json-schema <SCHEMA>
  --tools "" --strict-mcp-config --setting-sources project
  --no-session-persistence --disable-slash-commands
```

- La carpeta de trabajo es `<app_cache_dir>/anki`, que está vacía, así Claude Code no carga el `CLAUDE.md` de ningún proyecto.
- El prompt numera los subrayados `[1]`, `[2]`, ... y agrega la nota del lector cuando existe. Pide como máximo 2 tarjetas por subrayado, preguntas que no copien la respuesta y respuestas de una o dos frases en el idioma del libro.
- El schema pide `cards: [{source, front, back}]`, donde `source` es el número del subrayado. La app descarta las tarjetas cuyo `source` no corresponde a ningún subrayado.
- La app reusa `Enrich`, `EnrichEvent` y `enrich_stop` del enriquecimiento. El evento `done` lleva el texto "N tarjetas".

### Archivo

`anki::tsv` escribe `~/Downloads/KOReader Anki <fecha>.txt` con un libro o varios. Si el archivo ya existe, la app agrega `(2)`, `(3)` y así, para no pisar tarjetas que todavía no importaste.

```
#separator:tab
#html:true
#columns:guid	deck	Front	Back	tags
#guid column:1
#deck column:2
#tags column:5
kob-0b7f3c79-1	KOReader::Libro	¿Pregunta?	Respuesta<br><br><i>subrayado</i><br><small>Capítulo, p. 5</small>	koreader libro
```

- El GUID es el ID de bloque del subrayado (el mismo `^kob-` de Obsidian) más el número de tarjeta para ese subrayado.
- El mazo es `KOReader::<título>`. La app cambia `:` por espacio en el título para que no se formen submazos.
- Los campos escapan `&`, `<` y `>`, cambian los saltos de línea por `<br>` y los tabs por espacios.
- El archivo no fija el tipo de nota, porque en un perfil en español el tipo básico se llama "Básico" y no "Basic". El diálogo usa el tipo actual, que en un perfil nuevo es el básico.

### Interfaz

- La tarjeta de Claude en Subrayados suma el botón "Tarjetas Anki" al lado de "Enriquecer". Los dos usan los libros marcados o, si no hay ninguno marcado, el libro abierto.
- Mientras corre, la lista de libros muestra "Escribiendo tarjetas" y al terminar "N tarjetas".
- El aviso final muestra la ruta del archivo y los enlaces "Abrir en Anki" (`open -a Anki <archivo>`) y "Mostrar en Finder".

## Errores

- Si falta `claude`, el error es el mismo que en Enriquecer.
- Si Claude falla o no devuelve tarjetas para un libro, ese libro queda con error y la cola sigue. Si ningún libro dio tarjetas, la app no escribe el archivo.
- Si `open -a Anki` falla, el aviso dice que hay que instalar Anki y abrirlo una vez.

## Pruebas

- Unitarias en `anki.rs`: GUID estable e igual al ID de bloque, escape de HTML y tabs, descarte de `source` fuera de rango, error de Claude y formato del archivo con 5 columnas.
- `#[ignore]` `claude_anki`: corre Haiku sobre el libro del caché con menos subrayados (mínimo 3) y verifica que cada fila tiene 5 columnas y un GUID `kob-`.
- Sin probar: la importación en Anki, porque abrir Anki crea el perfil del usuario.

## Fuera de esta spec

AnkiConnect, `.apkg` y sincronizar con AnkiWeb. Anki sincroniza solo después de importar si el usuario inició sesión.
