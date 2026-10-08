// Keepr's product page: the moving parts. Plain JS, no build step.
(() => {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;

  // A deep link lands on its section at once; smooth scrolling is for clicks on the page.
  if (location.hash) {
    const target = document.querySelector(location.hash);
    if (target) {
      document.documentElement.style.scrollBehavior = "auto";
      target.scrollIntoView({ behavior: "instant", block: "start" });
      requestAnimationFrame(() => (document.documentElement.style.scrollBehavior = ""));
    }
  }

  // --- The tour: the window pans and zooms to the part each step talks about. ---
  const cam = document.getElementById("cam");
  const frames = cam ? [...cam.querySelectorAll(".frame")] : [];
  const steps = [...document.querySelectorAll(".step")];

  function focus(step) {
    const [x, y, w, h] = step.dataset.focus.split(" ").map(Number);
    const s = Math.min(100 / w, 100 / h, 2.6);
    // Put the rectangle's centre at the frame's centre, then keep the image covering the frame.
    const clamp = (v) => Math.min(0, Math.max(100 - 100 * s, v));
    const tx = clamp(50 - (x + w / 2) * s);
    const ty = clamp(50 - (y + h / 2) * s);
    const t = `translate(${tx}%, ${ty}%) scale(${s})`;
    for (const f of frames) {
      f.style.transform = t;
      f.classList.toggle("on", f.dataset.shot === step.dataset.shot);
    }
    for (const st of steps) st.classList.toggle("on", st === step);
  }

  if (steps.length && frames.length) {
    focus(steps[0]);
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) if (e.isIntersecting) focus(e.target);
      },
      { rootMargin: "-45% 0px -45% 0px", threshold: 0 },
    );
    steps.forEach((s) => io.observe(s));
  }

  // --- Only what changed: 72 pieces; the first backup stores them all, the rest store a few. ---
  const tiles = document.getElementById("tiles");
  const cap = document.getElementById("tilecap");
  if (tiles && cap) {
    // The pieces are in the HTML (all stored, for a page without this script); the loop redraws them.
    const els = [...tiles.querySelectorAll(".tile")];
    const N = els.length;
    // Later backups change a few pieces each: the same ones every loop, so it reads as a story.
    const rounds = [
      [5, 6, 23, 41, 58, 59],
      [23, 24, 66],
      [5, 12, 13, 30, 48],
      [41, 70],
    ];
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    let running = false,
      visible = false;

    const caption = (html) => {
      cap.innerHTML = html;
    };

    async function play() {
      if (running) return;
      running = true;
      while (visible) {
        tiles.classList.remove("reset");
        els.forEach((e) => (e.className = "tile"));
        caption(`Backup <span class="n">1</span> copies everything: <span class="n">72</span> pieces.`);
        for (let i = 0; i < N; i++) {
          els[i].classList.add("stored");
          if (!reduced && i % 2) await sleep(16);
        }
        await sleep(reduced ? 2400 : 1700);
        let n = 2;
        for (const r of rounds) {
          if (!visible) break;
          caption(
            `Backup <span class="n">${n}</span> stores only what changed: <span class="n">${r.length}</span> ${r.length === 1 ? "piece" : "pieces"}. It still restores as a complete copy.`,
          );
          for (const i of r) {
            els[i].classList.add("new");
            if (!reduced) await sleep(90);
          }
          await sleep(650);
          for (const i of r) els[i].classList.remove("new");
          await sleep(reduced ? 2400 : 1900);
          n++;
        }
        if (!visible) break;
        tiles.classList.add("reset");
        await sleep(900);
      }
      running = false;
    }

    new IntersectionObserver(
      ([e]) => {
        visible = e.isIntersecting;
        if (visible) play();
      },
      { threshold: 0.35 },
    ).observe(tiles);
  }

  // --- Encryption: the same lines, as the destination sees them. ---
  const scram = document.getElementById("scram");
  if (scram) {
    const plain = scram.textContent;
    const glyphs = "0123456789abcdefABCDEF+/=xqzkwvjy";
    const rnd = () => glyphs[(Math.random() * glyphs.length) | 0];
    const scrambleAll = () => plain.replace(/[^\n]/g, () => rnd());
    const io = new IntersectionObserver(
      ([e]) => {
        if (!e.isIntersecting) return;
        io.disconnect();
        if (reduced) {
          scram.textContent = scrambleAll();
          return;
        }
        // Sweep left to right, line by line, over about a second.
        const lines = plain.split("\n");
        const width = Math.max(...lines.map((l) => l.length));
        let col = 0;
        const tick = () => {
          col++;
          scram.textContent = lines
            .map((l) =>
              l
                .split("")
                .map((c, i) => (i < col ? rnd() : c))
                .join(""),
            )
            .join("\n");
          if (col <= width) setTimeout(tick, 34);
          else {
            // Settle: keep flickering a little, like noise does, then stop.
            let k = 0;
            const settle = setInterval(() => {
              scram.textContent = scrambleAll();
              if (++k > 4) clearInterval(settle);
            }, 120);
          }
        };
        tick();
      },
      { threshold: 0.6 },
    );
    io.observe(scram);
  }

  // --- Menu bar: the panel opens when it comes into view. ---
  const mac = document.querySelector(".mac");
  if (mac) {
    const io = new IntersectionObserver(
      ([e]) => {
        if (e.isIntersecting) {
          mac.classList.add("on");
          io.disconnect();
        }
      },
      { threshold: 0.4 },
    );
    io.observe(mac);
  }

  // --- Screenshots: the shimmer behind each one stops when it has loaded (or failed). ---
  for (const box of document.querySelectorAll("[data-shimmer]")) {
    const img = box.querySelector("img");
    const done = () => box.classList.add("loaded");
    if (!img || img.complete) done();
    else {
      img.addEventListener("load", done, { once: true });
      img.addEventListener("error", done, { once: true });
    }
  }

  // --- The latest version, from GitHub, when it answers. ---
  const v = document.getElementById("version");
  if (v) {
    fetch("https://api.github.com/repos/waynetd777/keepr/releases/latest", { headers: { Accept: "application/vnd.github+json" } })
      .then((r) => (r.ok ? r.json() : null))
      .then((j) => {
        if (j && j.tag_name) v.textContent = `Version ${j.tag_name.replace(/^v/, "")}`;
      })
      .catch(() => {});
  }
})();
