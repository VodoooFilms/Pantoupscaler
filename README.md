<p align="center">
  <img src="assets/icon.png" width="128" height="128" alt="Panto Upscaler">
</p>

<h1 align="center">Panto Upscaler</h1>

<p align="center">
  Escalador de vídeo local con <strong>FFmpeg + Lanczos</strong> para macOS, Windows y Linux.
  <br>
  Sin subir nada a la nube.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-yellow.svg" alt="Licencia MIT"></a>
  <a href="https://v2.tauri.app"><img src="https://img.shields.io/badge/Tauri-2-24C8DB?logo=tauri&logoColor=white" alt="Tauri 2"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.77%2B-000000?logo=rust&logoColor=white" alt="Rust 1.77+"></a>
</p>

<p align="center">
  <a href="https://github.com/VodoooFilms/Pantoupscaler">github.com/VodoooFilms/Pantoupscaler</a>
</p>

La app mantiene el lenguaje visual de [Pantoraya](https://github.com/VodoooFilms/pantoraya):
ventana compacta, tipografía del sistema, temas claro/oscuro y español/inglés.
La diferencia es el flujo: **dos paneles iguales, entrada arriba y salida abajo**,
con resolución, peso y ajuste de encuadre siempre visibles.

## Por qué Tauri y no Electron

El trabajo pesado lo hace FFmpeg, no la interfaz. Tauri aporta lo que Electron no:
un backend nativo en Rust y un binario final mucho más pequeño, reutilizando la
webview del sistema. El frontend sigue siendo HTML/CSS/JS, así que la estética de
Pantoraya se conserva sin reescribir la UI en un toolkit nativo.

| | Electron | Tauri v2 (esta app) |
| --- | --- | --- |
| Backend | Node.js | Rust |
| UI | Chromium embebido | WebView del sistema |
| Peso base | ~120 MB | ~10-15 MB + FFmpeg |
| FFmpeg | Proceso hijo | Proceso hijo |

## Cómo escala

Ejemplo real de la app: `832 × 480` → `1920 × 1080`.

- **Recortar** (por defecto) escala a `1920 × 1108` y recorta 14 px arriba y
  abajo, sin barras.
- **Barras negras** escala a `1872 × 1080` y rellena 24 px negros a cada lado.

En modo **Barras negras**, el cálculo es:

1. Se calcula la escala que cabe dentro del destino conservando la proporción:
   `min(1920 / 832, 1080 / 480) = 2.25`.
2. El vídeo queda en `1872 × 1080`.
3. FFmpeg añade `(1920 - 1872) / 2 = 24 px` negros a cada lado.

Filtro equivalente:

```text
scale=1872:1080:flags=lanczos+accurate_rnd+full_chroma_int,
pad=1920:1080:(ow-iw)/2:(oh-ih)/2:black,
setsar=1
```

Modos disponibles:

- **Recortar** (`crop`, por defecto): llena el destino y recorta el excedente
  centrado, sin barras.
- **Barras negras** (`pad`): conserva todo el encuadre y rellena con negro.
- **Estirar** (`stretch`): fuerza las dimensiones destino sin conservar proporción.

Salida: MP4 H.264 (`libx264`, `CRF 14`, preset `veryslow`, `yuv420p`,
`+faststart`). La codificación es deliberadamente calidad-primero: `veryslow`
aprovecha mejor cada bit y `CRF 14` mantiene el escalado de Lanczos
visualmente transparente, a costa de un tiempo de codificación mayor. El audio
se copia sin recomprimir cuando el contenedor lo permite (AAC, MP3, AC3, E-AC3,
ALAC); si no, se recodifica a AAC 192 kbps.

## Estructura

```text
ui/                         Frontend (HTML, CSS y JS sin bundler)
  index.html
  styles.css
  app.js
  mock.js                   Puente falso para previsualizar la UI en un navegador
src-tauri/
  src/main.rs               Entrada de la app
  src/lib.rs                Builder de Tauri y registro de comandos
  src/commands.rs           Comandos IPC
  src/ffmpeg.rs             Resolución de FFmpeg, sondeo, miniaturas, conversión
  src/plan.rs               Cálculo de resolución, barras, filtros y estimación
  src/state.rs              Estado compartido y cancelación
  tests/convert.rs          Prueba de integración real con FFmpeg
  tauri.conf.json           Ventana, CSP, recursos y empaquetado
scripts/prepare-ffmpeg.sh   Prepara el binario para empaquetar
```

## Requisitos

- Rust 1.77+ y las herramientas de compilación del sistema.
- Node.js solo para el CLI de Tauri (`npm run dev` / `npm run build`).
- FFmpeg en el `PATH` para desarrollo.

En macOS:

```bash
brew install ffmpeg
```

## Instalación

Descarga el `.dmg` desde [Releases](https://github.com/VodoooFilms/Pantoupscaler/releases),
ábrelo y arrastra **Panto Upscaler** a Aplicaciones. La app incluye su propio
FFmpeg: funciona sin conexión y sin instalar nada más.

## Desarrollo

```bash
git clone https://github.com/VodoooFilms/Pantoupscaler.git
cd Pantoupscaler
npm install
npm run dev
```

Si prefieres no usar Node:

```bash
cargo run --manifest-path src-tauri/Cargo.toml
```

La UI también se puede abrir directamente en un navegador. `ui/mock.js` simula
los comandos de Tauri con un vídeo de demostración:

```bash
open ui/index.html
```

## Uso

- Arrastra un vídeo sobre la tarjeta de entrada o pulsa `⌘O` / `Ctrl+O`.
- Elige resolución (`720p`, `1080p`, `1440p` o `4K`) y ajuste de encuadre.
- La salida se guarda junto al vídeo original. El icono de carpeta del panel de
  salida permite elegir otra ubicación.
- Al terminar, `Mostrar en Finder` revela el archivo y `Escalar otro` reinicia
  el flujo.

## Pruebas

```bash
npm test
```

La prueba de integración genera un clip `832 × 480` con FFmpeg, lo escala a
`1080p` y verifica que la salida mide `1920 × 1080`, conserva el audio y calcula
barras de `24 px`.

## Empaquetado

```bash
npm run prepare:engines
npm run build
```

`prepare:engines` busca FFmpeg en este orden:

1. `src-tauri/resources/ffmpeg/ffmpeg`
2. `build/ffmpeg/ffmpeg` (compilación de Pantoraya)
3. `/Applications/Pantoraya.app/Contents/Resources/ffmpeg/ffmpeg`
4. `ffmpeg` del `PATH`

El resultado queda en `src-tauri/target/release/bundle/macos/Panto Upscaler.app`
(y el `.dmg` correspondiente).

En tiempo de ejecución, Rust busca el binario en los recursos de la app, en
`src-tauri/resources/ffmpeg/`, en `PANTO_FFMPEG` y finalmente en el `PATH`.

Para distribuir en macOS conviene usar un FFmpeg **estático**. El binario de
Pantoraya ya lo es; un FFmpeg de Homebrew enlaza `libav*` y no funcionará si se
copia solo el ejecutable.

## Contribuciones

Las contribuciones son bienvenidas. Abre un issue para proponer una mejora o
envía un pull request con el cambio y sus pruebas.

## Licencia

Código MIT © 2026 VodoooFilms. Consulta [LICENSE](LICENSE).
FFmpeg, x264 y LAME conservan sus licencias; consulta
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
