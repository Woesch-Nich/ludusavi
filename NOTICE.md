# Credits and License

This project is a fork of **Ludusavi**.

## Upstream

- Original project: [mtkennerly/ludusavi](https://github.com/mtkennerly/ludusavi)
- Original author: **Matthew T. Kennerly (mtkennerly)**
- Upstream contributors: see the upstream repository history and contributors graph.
- License: **MIT**. The upstream copyright notice and permission notice are retained in [LICENSE](LICENSE).

Ludusavi uses the [Ludusavi Manifest](https://github.com/mtkennerly/ludusavi-manifest),
with data primarily sourced from [PCGamingWiki](https://www.pcgamingwiki.com/wiki/Home).
Those projects and their contributors deserve credit for the game-save data set.

## Fork

- Fork repository: [Woesch-Nich/ludusavi](https://github.com/Woesch-Nich/ludusavi)
- Fork maintainer: **Woesch-Nich**
- Fork modifications copyright (c) 2026 Woesch-Nich and contributors.
- The fork modifications are distributed under the same **MIT** license as upstream.

### What this fork adds

- A built-in daily automatic backup schedule in the GUI settings.
- On Windows, the schedule is managed through a real Windows scheduled task, so the backup can run even when the Ludusavi GUI is closed.
- The scheduled backup uses the existing Ludusavi configuration and backup behavior; it does not require a separate external script.

This fork is not affiliated with or endorsed by the upstream Ludusavi project.
For the official upstream project, use [mtkennerly/ludusavi](https://github.com/mtkennerly/ludusavi).