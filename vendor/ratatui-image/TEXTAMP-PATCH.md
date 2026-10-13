This is ratatui-image 9.0.0 (MIT), vendored from the locked crates.io source.
Only src/protocol/kitty.rs differs from that release.

Backport: wrap each Kitty graphics chunk in its own tmux passthrough message,
rather than wrapping the entire transfer. Large covers otherwise exceed tmux's
passthrough buffer while small thumbnails can still work. Preserve original
pixel data, image IDs, chunk ordering and continuation markers.

Upstream implementation: https://github.com/ratatui/ratatui-image/blob/master/src/protocol/kitty.rs
Upstream changelog: https://github.com/ratatui/ratatui-image/blob/master/CHANGELOG.md

Remove this patch when upgrading to an upstream release containing that fix.
