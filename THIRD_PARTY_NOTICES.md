# Avisos de terceros

## FFmpeg

Panto Upscaler invoca FFmpeg como proceso externo para leer, escalar y
codificar vídeo. FFmpeg se distribuye bajo la GNU General Public License
(GPL) versión 2 o posterior, según la compilación utilizada.

- Sitio oficial: https://ffmpeg.org
- Código fuente: https://ffmpeg.org/download.html
- Licencia: https://ffmpeg.org/legal.html

Si empaquetas la aplicación con el binario `src-tauri/resources/ffmpeg/ffmpeg`,
debes acompañar la distribución con esta licencia y ofrecer el código fuente
correspondiente de FFmpeg.

## x264

Las compilaciones de FFmpeg que incluyen `libx264` incorporan x264, distribuido
bajo GPL. Código fuente: https://code.videolan.org/videolan/x264

## LAME

Si el binario de FFmpeg incluye el codificador MP3 de LAME, se aplica la
licencia LGPL de LAME. Código fuente: https://lame.sourceforge.io
