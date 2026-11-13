// Скрипт живёт внутри страницы SoundCloud. Он ничего не добавляет на сайт,
// только раз в 2 секунды читает нижнюю панель плеера и отправляет данные
// в приложение командой update_presence.
(() => {
  // Скрипт внедряется во все фреймы страницы, а плеер есть только в основном:
  // из iframe он слал бы «ничего не играет» и статус мигал бы.
  if (window.top !== window.self) return;

  if (window.__emocloudPresence) return;
  window.__emocloudPresence = true;

  const POLL_MS = 2000;

  let prevPosition = null;
  let prevPositionAt = 0;

  const text = (node) =>
    node ? (node.textContent || "").replace(/\s+/g, " ").trim() : "";

  // В разметке SoundCloud название дублируется: скрытый span «Current track: …»
  // плюс видимый. Надёжнее взять атрибут title, а текст — только как запасной вариант.
  const labelOf = (node) => {
    if (!node) return "";
    const title = (node.getAttribute("title") || "").trim();
    if (title) return title;
    const visible = node.querySelector('[aria-hidden="true"]');
    return text(visible || node);
  };

  const toSeconds = (value) => {
    if (!value) return null;
    const parts = value.split(":").map((part) => parseInt(part, 10));
    if (!parts.length || parts.some((part) => Number.isNaN(part))) return null;
    return parts.reduce((total, part) => total * 60 + part, 0);
  };

  const artworkUrl = (badge) => {
    const avatar = badge.querySelector(".playbackSoundBadge__avatar");
    if (!avatar) return null;

    const image = avatar.querySelector("img");
    let url = (image && (image.currentSrc || image.src)) || "";

    if (!url) {
      for (const node of [avatar, ...avatar.querySelectorAll("*")]) {
        const background = getComputedStyle(node).backgroundImage || "";
        const match = background.match(/url\(["']?(.+?)["']?\)/);
        if (match && match[1]) {
          url = match[1];
          break;
        }
      }
    }

    if (!url) return null;
    // Мелкую обложку просим в большем размере — Discord покажет её крупнее.
    return url.replace(/-t\d+x\d+(\.[a-z]+)(\?.*)?$/i, "-t500x500$1");
  };

  // true — играет, false — пауза, null — не смогли понять по кнопке.
  const buttonPlaying = () => {
    const button = document.querySelector(".playControls__play");
    if (!button) return null;

    const classes = String(button.className || "");
    const label = button.getAttribute("aria-label") || "";

    if (/\bplaying\b/.test(classes)) return true;
    if (/pause|пауза/i.test(label)) return true;
    if (/play|воспроизв/i.test(label)) return false;
    return null;
  };

  const readTrack = () => {
    const badge = document.querySelector(".playbackSoundBadge");
    if (!badge) return null;

    const titleLink = badge.querySelector(".playbackSoundBadge__titleLink");
    const title = labelOf(titleLink);
    if (!title) return null;

    const passed = document.querySelector(
      '.playbackTimeline__timePassed span[aria-hidden="true"]'
    );
    const duration = document.querySelector(
      '.playbackTimeline__duration span[aria-hidden="true"]'
    );

    const position = toSeconds(text(passed));
    const total = toSeconds(text(duration));

    // Если таймер тикает, трек точно играет — даже если кнопку не нашли.
    const now = Date.now();
    let advancing = false;
    if (position !== null && prevPosition !== null && now - prevPositionAt > 500) {
      advancing = position > prevPosition + 0.4;
    }
    if (position !== null) {
      prevPosition = position;
      prevPositionAt = now;
    } else {
      prevPosition = null;
    }

    const fromButton = buttonPlaying();

    return {
      title,
      artist: labelOf(badge.querySelector(".playbackSoundBadge__lightLink")),
      artwork: artworkUrl(badge),
      url: titleLink && titleLink.href ? titleLink.href.split("?")[0] : null,
      playing: fromButton === null ? advancing : fromButton,
      position,
      duration: total,
    };
  };

  const start = () => {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== "function") {
      // Tauri внедряет свой IPC чуть позже — подождём его.
      setTimeout(start, 250);
      return;
    }

    const send = () => {
      let track = null;
      try {
        track = readTrack();
      } catch (_) {
        track = null;
      }
      internals.invoke("update_presence", { track }).catch(() => {});
    };

    send();
    setInterval(send, POLL_MS);
  };

  start();
})();
