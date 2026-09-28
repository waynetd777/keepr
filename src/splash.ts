// The splash index.html paints before React starts. Kept out of main.tsx so that importing it
// doesn't re-run the entry point on a hot reload.

/// How long the splash stays up at minimum, so a fast start reads as a deliberate opening.
const MIN_SPLASH_MS = 900;
const splashShownAt = Date.now();

/// Fade out the splash once there's a real screen to replace it.
export function hideSplash() {
  const el = document.getElementById("splash");
  // Screenshot mode's splash scene keeps it up.
  if (!el || el.dataset.going || document.documentElement.dataset.keepSplash) return;
  const wait = Math.max(0, MIN_SPLASH_MS - (Date.now() - splashShownAt));
  el.dataset.going = "1";
  setTimeout(() => {
    el.classList.add("gone");
    setTimeout(() => el.remove(), 250);
  }, wait);
}
