/* Dev-only bridge so the interface can be previewed in a normal browser.
   Inside Tauri this file does nothing because window.__TAURI__ already exists. */
(() => {
  if (window.__TAURI__) return;
  if (location.protocol === "tauri:" || /tauri\.localhost$/.test(location.hostname)) return;

  const listeners = new Map();
  let cancelled = false;
  let busy = false;

  const svg = `
    <svg xmlns="http://www.w3.org/2000/svg" width="832" height="480" viewBox="0 0 832 480">
      <defs>
        <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#2b3a55"/>
          <stop offset="0.55" stop-color="#7d6a86"/>
          <stop offset="1" stop-color="#e0a273"/>
        </linearGradient>
        <linearGradient id="ground" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stop-color="#202a35"/>
          <stop offset="1" stop-color="#0c1118"/>
        </linearGradient>
      </defs>
      <rect width="832" height="480" fill="url(#sky)"/>
      <circle cx="618" cy="126" r="46" fill="#f3cf8a" opacity="0.9"/>
      <path d="M0 328 L150 246 L286 300 L430 212 L590 312 L720 250 L832 306 L832 480 L0 480 Z" fill="#1a232e"/>
      <path d="M0 380 L124 322 L260 364 L404 300 L558 372 L700 318 L832 366 L832 480 L0 480 Z" fill="url(#ground)"/>
      <text x="416" y="258" text-anchor="middle" fill="rgba(255,255,255,0.82)"
        font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-size="58"
        font-weight="700" letter-spacing="8">CORTO FINAL</text>
      <text x="416" y="294" text-anchor="middle" fill="rgba(255,255,255,0.45)"
        font-family="Helvetica Neue, Helvetica, Arial, sans-serif" font-size="16"
        letter-spacing="4">ESCENA 04 · 832 × 480</text>
    </svg>`;

  const thumbnail = `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

  const file = {
    path: "/Users/demo/CORTOFINAL.mp4",
    name: "CORTOFINAL.mp4",
    size: 18_400_000,
    width: 832,
    height: 480,
    duration: 42.5,
    fps: 24,
    codec: "h264",
    audioCodec: "aac",
    audioKbps: 192,
    hasAudio: true,
    thumbnail,
  };

  const targets = {
    "720": { width: 1280, height: 720, label: "720p" },
    "1080": { width: 1920, height: 1080, label: "1080p" },
    "1440": { width: 2560, height: 1440, label: "1440p" },
    "2160": { width: 3840, height: 2160, label: "4K" },
  };

  const floorEven = (value) => Math.max(2, Math.floor(value) - (Math.floor(value) % 2));
  const ceilEven = (value) => Math.max(2, Math.ceil(value) + (Math.ceil(value) % 2));

  function computePlan(width, height, duration, audioKbps, targetId, mode) {
    const target = targets[targetId] || targets["1080"];
    const plan = {
      targetId: targetId in targets ? targetId : "1080",
      label: target.label,
      mode,
      targetWidth: target.width,
      targetHeight: target.height,
      scaledWidth: target.width,
      scaledHeight: target.height,
      barLeft: 0,
      barRight: 0,
      barTop: 0,
      barBottom: 0,
      cropLeft: 0,
      cropTop: 0,
      estimatedBytes: duration > 0
        ? Math.round(duration * (target.height >= 2160 ? 32000 : target.height >= 1440 ? 16000 : target.height >= 1080 ? 8000 : 4500) * 1000 / 8 * 1.02)
        : 0,
      valid: width > 0 && height > 0,
    };

    if (!plan.valid) return plan;

    if (mode === "crop") {
      const scale = Math.max(target.width / width, target.height / height);
      plan.scaledWidth = Math.max(target.width, ceilEven(width * scale));
      plan.scaledHeight = Math.max(target.height, ceilEven(height * scale));
      plan.cropLeft = Math.round((plan.scaledWidth - target.width) / 2);
      plan.cropTop = Math.round((plan.scaledHeight - target.height) / 2);
    } else if (mode === "stretch") {
      plan.scaledWidth = target.width;
      plan.scaledHeight = target.height;
    } else {
      const scale = Math.min(target.width / width, target.height / height);
      plan.scaledWidth = Math.min(target.width, floorEven(width * scale));
      plan.scaledHeight = Math.min(target.height, floorEven(height * scale));
      const horizontal = target.width - plan.scaledWidth;
      const vertical = target.height - plan.scaledHeight;
      plan.barLeft = Math.floor(horizontal / 2);
      plan.barRight = horizontal - plan.barLeft;
      plan.barTop = Math.floor(vertical / 2);
      plan.barBottom = vertical - plan.barTop;
    }

    return plan;
  }

  const emit = (event, payload) => {
    for (const callback of listeners.get(event) || []) callback({ payload });
  };

  const sleep = (milliseconds) => new Promise((resolve) => setTimeout(resolve, milliseconds));

  async function invoke(command, args = {}) {
    switch (command) {
      case "select_video":
      case "inspect_video":
        return { ...file };
      case "compute_plan":
        return computePlan(
          args.width,
          args.height,
          args.duration,
          args.audioKbps,
          args.target,
          args.mode,
        );
      case "choose_output_folder":
        await sleep(80);
        return null;
      case "start_conversion": {
        if (busy) throw "BUSY: ya hay una conversión en curso.";
        busy = true;
        cancelled = false;
        const request = args.request || {};
        const plan = computePlan(file.width, file.height, file.duration, file.audioKbps, request.targetId, request.mode);
        const fileStem = file.name.replace(/\.[^.]+$/, "");
        const outputName = `${fileStem}_${plan.label.toLowerCase()}.mp4`;

        for (let percent = 0; percent <= 96; percent += 6) {
          if (cancelled) {
            busy = false;
            throw "CANCELLED: conversión cancelada.";
          }
          emit("conversion-progress", { percent, elapsed: percent / 20 });
          await sleep(110);
        }

        if (cancelled) {
          busy = false;
          throw "CANCELLED: conversión cancelada.";
        }

        emit("conversion-progress", { percent: 100, elapsed: 5.2 });
        busy = false;
        return {
          outputPath: `/Users/demo/${outputName}`,
          outputName,
          outputBytes: Math.round(file.size * 2.4),
          inputBytes: file.size,
          thumbnail,
          elapsedSeconds: 5.2,
          plan,
        };
      }
      case "cancel_conversion":
        cancelled = true;
        return true;
      case "show_in_folder":
        return null;
      case "ffmpeg_status":
        return { available: true, path: "/opt/homebrew/bin/ffmpeg" };
      default:
        throw new Error(`Comando desconocido: ${command}`);
    }
  }

  window.__TAURI__ = {
    core: { invoke },
    event: {
      async listen(event, callback) {
        if (!listeners.has(event)) listeners.set(event, new Set());
        listeners.get(event).add(callback);
        return () => listeners.get(event)?.delete(callback);
      },
    },
  };
  window.__PANTO_DEMO__ = true;
})();
