# Embedded YouTube signature solver

`lib.min.js` and `core.min.js` are the unmodified solver assets from the official
[`yt-dlp-ejs` 0.8.0 wheel](https://pypi.org/project/yt-dlp-ejs/0.8.0/).

Wheel: `yt_dlp_ejs-0.8.0-py3-none-any.whl`.
SHA-256 verified against PyPI metadata:
`79300e5fca7f937a1eeede11f0456862c1b41107ce1d726871e0207424f4bdb4`.

EJS is Unlicense (`LICENSE`). The library embeds meriyah 6.1.4 (ISC) and
astring 1.9.0 (MIT); their full notices remain in the JS header and
`DEPENDENCIES.txt`. `LICENSE-QuickJS.txt` covers the QuickJS engine bundled by
rquickjs-sys 0.14.0. `LICENSE-rquickjs.txt` covers its Rust bindings.
Include these notices when distributing Tonal.

Tonal's Rust orchestration is independently implemented. No Rust source from
ytm-rs or Tubefast is included. The QuickJS context exposes no filesystem,
network, process, cookie or account bindings. Only public player preparation
is persisted; signed media links and music remain outside this disk cache.
