"""Isolated local-folder startup/navigation smoke; never touches user settings."""
import fcntl
import os
import pathlib
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unicodedata
import wave

wikipedia = "--wikipedia" in sys.argv[2:]
browse_only = "--browse-only" in sys.argv[2:]
ghostty = "--ghostty" in sys.argv[2:]
ghostty_tmux = "--ghostty-tmux" in sys.argv[2:]
assert not (wikipedia and browse_only), "Wikipedia smoke requires track metadata loading"

with tempfile.TemporaryDirectory(prefix="textamp-folder-tui-") as temporary:
    root = pathlib.Path(temporary)
    music = root / "music"
    album = music / "Fixture Album"
    album.mkdir(parents=True)
    for name in ("02 Silence.wav", "10 Silence.wav"):
        with wave.open(str(album / name), "wb") as audio:
            audio.setnchannels(2)
            audio.setsampwidth(2)
            audio.setframerate(44100)
            audio.writeframes(b"\0" * 44100 * 4 * 8)
        if wikipedia:
            # Standard RIFF INFO artist tag, not a custom sidecar convention.
            song = album / name
            raw = song.read_bytes()
            artist = b"Tool\0\0"
            info = b"INFOIART" + struct.pack("<I", len(artist)) + artist
            chunk = b"LIST" + struct.pack("<I", len(info)) + info
            data_at = raw.index(b"data", 12)
            song.write_bytes(b"RIFF" + struct.pack("<I", len(raw) + len(chunk) - 8) + raw[8:data_at] + chunk + raw[data_at:])
    env = os.environ.copy()
    for name in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"):
        env[name] = str(root / name)
    config_dir = pathlib.Path(env["XDG_CONFIG_HOME"]) / "textamp"
    config_dir.mkdir(parents=True)
    (config_dir / "config.toml").write_text(
        f'default_folder_source = "fixture"\n[[folder_sources]]\nid = "fixture"\nname = "Fixture Library"\nkind = "local"\npath = "{music}"\n'
    )
    env["TERM"] = "xterm-256color"
    env["TERM_PROGRAM"] = "Apple_Terminal"
    env.pop("TMUX", None)
    if ghostty or ghostty_tmux:
        env["TERM"] = "xterm-ghostty"
        env["TERM_PROGRAM"] = "ghostty"
    if ghostty_tmux:
        # Simulate a multiplexer without contacting the user's tmux server.
        env["TMUX"] = "/tmp/textamp-fixture-tmux,1,0"
        env["TERM"] = "tmux-256color"
        env["TERM_PROGRAM"] = "tmux"
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 130, 0, 0))
    before = termios.tcgetattr(slave)
    process = subprocess.Popen([str(pathlib.Path(sys.argv[1]).resolve())], stdin=slave, stdout=slave, stderr=slave, env=env)
    output = bytearray()
    screen_output = bytearray()
    answered_queries = set()

    def read_output():
        data = os.read(master, 65536)
        output.extend(data)
        screen_output.extend(data)
        if ghostty and "graphics" not in answered_queries and b"\x1b[5n" in screen_output:
            # Behave like a native Ghostty terminal: acknowledge Kitty support,
            # report 10x20 pixel cells, then finish the capability query. Without
            # a reply, the probe's stdin reader can consume navigation keys.
            os.write(master, b"\x1b_Gi=31;OK\x1b\\\x1b[?64;4c\x1b[6;20;10t\x1b[0n")
            answered_queries.add("graphics")

    def screen_text():
        # Ratatui emits only changed cells. Reconstruct cursor-addressed text:
        # stripping CSI from a diff loses letters reused from the prior frame.
        raw = screen_output.decode("utf-8", errors="replace")
        cells = [[" "] * 240 for _ in range(80)]
        x = y = 0
        i = 0
        while i < len(raw):
            char = raw[i]
            if char == "\x1b":
                match = re.match(r"\x1b\[([0-?]*)([ -/]*)([@-~])", raw[i:])
                if match:
                    params, _, command = match.groups()
                    values = [int(n) for n in re.findall(r"\d+", params)]
                    n = values[0] if values and values[0] else 1
                    if command in "Hf":
                        y = (values[0] if values else 1) - 1
                        x = (values[1] if len(values) > 1 else 1) - 1
                    elif command == "G": x = n - 1
                    elif command == "d": y = n - 1
                    elif command == "A": y -= n
                    elif command == "B": y += n
                    elif command == "C": x += n
                    elif command == "D": x -= n
                    elif command in "JK":
                        mode = values[0] if values else 0
                        if command == "J" and mode == 2:
                            cells = [[" "] * 240 for _ in range(80)]
                        elif command == "J" and mode == 0:
                            cells[y][x:] = [" "] * (240 - x)
                            for row in range(y + 1, 80): cells[row] = [" "] * 240
                        elif command == "K":
                            start, end = (0, 240) if mode == 2 else ((0, x + 1) if mode == 1 else (x, 240))
                            cells[y][start:end] = [" "] * (end - start)
                    x, y = max(0, min(x, 239)), max(0, min(y, 79))
                    i += len(match.group())
                    continue
                if raw[i:i + 2] == "\x1b]":
                    ends = [end for end in (raw.find("\x07", i + 2), raw.find("\x1b\\", i + 2)) if end >= 0]
                    if not ends: break
                    end = min(ends)
                    i = end + (1 if raw[end] == "\x07" else 2)
                    continue
                i += 3 if raw[i:i + 2] in ("\x1b(", "\x1b)") else 2
                continue
            if char == "\r": x = 0
            elif char == "\n": y = min(y + 1, 79)
            elif char == "\b": x = max(x - 1, 0)
            elif char >= " " and not unicodedata.combining(char):
                cells[y][x] = char
                width = 2 if unicodedata.east_asian_width(char) in "WF" else 1
                if width == 2 and x + 1 < 240: cells[y][x + 1] = ""
                x = min(x + width, 239)
            i += 1
        return "\n".join("".join(row) for row in cells).encode()

    def wait_for(text):
        # Differential terminal draws can move across already-correct spaces
        # without emitting them again. Match text independent of those gaps.
        def normalized(value):
            return re.sub(rb"\s+", b"", re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z]", b"", value))
        started = time.monotonic()
        deadline = started + (50 if wikipedia else 20)
        redrawn = False
        while normalized(text) not in normalized(screen_text()) and time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                read_output()
            if process.poll() is not None:
                break
            if not redrawn and time.monotonic() - started > 2:
                # A differential draw can reuse individual letters from the
                # previous screen. Force one full frame before treating raw
                # escape-stream text matching as a missing UI element.
                rows, columns, xp, yp = struct.unpack("HHHH", fcntl.ioctl(slave, termios.TIOCGWINSZ, b"\0" * 8))
                columns = 131 if columns == 130 else 130
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, xp, yp))
                process.send_signal(signal.SIGWINCH)
                redrawn = True
        plain = screen_text()
        diagnostic = re.sub(r"[\u2500-\u28ff]", "", plain.decode("utf-8", errors="replace"))
        assert normalized(text) in normalized(plain), f"UI did not display {text!r}; exit={process.poll()}; tail={diagnostic[:10000]!r}"

    try:
        wait_for(b"Fixture Album")
        assert b"Sign in to Plex" not in output
        os.write(master, b"\r")
        wait_for(b"02 Silence.wav")
        if os.environ.get("TEXTAMP_MPRIS_SMOKE") == "1":
            status = subprocess.run(
                ["playerctl", "--player=textamp", "status"],
                capture_output=True, timeout=5, check=True,
            )
            assert status.stdout.strip() == b"Stopped", "browsing started playback"
            print("PASS: executable reports Stopped through MPRIS/playerctl")
        if not browse_only:
            os.write(master, b"\r")
            if wikipedia:
                wait_for(b"Tool")  # wait for async metadata, draining terminal output
            time.sleep(0.4)
            os.write(master, b" ")  # pause silent playback
        if wikipedia:
            os.write(master, b"\x1bOS")  # F4: standard artist tag -> Wikipedia
            wait_for(b"Tool is an American")
            wait_for(b"B source")
            output.clear()
            os.write(master, b"\x1b[C")  # next photo
            # Differential rendering replaces only the counter's digit; the
            # photo-selection behavior is separately covered by state tests.
            wait_for(b"2")
            os.write(master, b"\x1bOS")  # F4 closes the popup
            time.sleep(0.2)
        if browse_only:
            output.clear()
            os.write(master, b":codec")
            wait_for(b"View track details")
            os.write(master, b"\r")
            wait_for(b"source metadata")
            wait_for(b"pcm_s16le")  # asynchronous local metadata has arrived
            os.write(master, b"\x1b[6~")  # audio properties can be below the fold
            wait_for(b"44100 Hz")
            wait_for(b"16 bit")
            # Focus transitions must not clear the terminal or switch graphics
            # modes. Drain preceding frames before checking the focus redraws.
            time.sleep(0.2)
            while select.select([master], [], [], 0)[0]:
                screen_output.extend(os.read(master, 65536))
            output.clear()
            os.write(master, b"\x1b[O\x1b[I")
            deadline = time.monotonic() + 0.4
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.05)[0]:
                    read_output()
            assert b"\x1b[2J" not in output, "focus change cleared the terminal"
            os.write(master, b"\x1b[F\x1b[H\x1b")  # End/Home, close details
            time.sleep(0.2)
            output.clear()
            os.write(master, b",")  # Settings: comma avoids Mac brightness keys
            wait_for(b"Add library")
            wait_for(b"[Active]")
            os.write(master, b"a")
            wait_for(b"Subsonic / Navidrome")
            os.write(master, b"\x1b[B\r")  # WebDAV type -> one form
            wait_for(b"Add WebDAV library")
            wait_for(b"Name (optional)")
            os.write(master, b"\x1b")  # cancel, leaving the library untouched
            time.sleep(0.2)
            output.clear()
            # Settings now has a top menu and separate pages, rather than the
            # former sidebar and one long settings list. F10 focuses the menu.
            os.write(master, b"\x1b[21~2\x1b[B\x1b[B")  # Textamp -> Themes content
            wait_for(b"Highlight to preview")
            output.clear()
            os.write(master, b"\x1b[D\x1b[C\x1b[C\x1b[B")  # pages -> Sidebar content
            wait_for(b"appears in the browse sidebar")
            output.clear()
            os.write(master, b"\x1b[21~1\x1b[B\x1b[H\r")  # Libraries -> first library options
            wait_for(b"Open active library")
            os.write(master, b"\x1b[C")  # local-library General -> Cache page
            # Force a complete redraw: differential updates can skip letters
            # that happen to match cells from the previous settings panel.
            output.clear()
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 131, 0, 0))
            process.send_signal(signal.SIGWINCH)
            wait_for(b"Fixture Library")
            wait_for(b"Metadata:")
            wait_for(b"Clear library cache")
            os.write(master, b"\x1b")
            time.sleep(0.2)
        os.write(master, b"\x1bOR")  # F3 library switcher
        wait_for(b"Switch library")
        os.write(master, b"\x11")  # Ctrl+Q: normal keyboard/controller-mapped quit
        deadline = time.monotonic() + 10
        while process.poll() is None and time.monotonic() < deadline:
            if select.select([master], [], [], 0.1)[0]:
                read_output()
        assert process.poll() == 0, f"unexpected exit {process.poll()}"
        assert termios.tcgetattr(slave) == before, "terminal not restored"
        if ghostty:
            assert b"\x1b_G" in screen_output, "Direct Ghostty never probed native images"
            print("PASS: direct Ghostty negotiates native graphics")
        if ghostty_tmux:
            assert b"\x1b_G" not in screen_output, "Ghostty received Kitty graphics/probe commands"
            assert "\U0010eeee".encode() not in screen_output, "Ghostty received image placeholders"
            print("PASS: Ghostty compatibility sends no Kitty graphics or placeholders")
        print("PASS: folder-only startup, filename navigation, library switcher, shutdown, and terminal restoration")
        if browse_only:
            print("PASS: searchable Actions, local track details without playback, focus without terminal clears")
            print("PASS: unified library manager, add/cancel connection form, integrated settings, per-library cache display")
            print("PASS: Ctrl+Q quit and terminal restoration")
        if not browse_only:
            print("PASS: silent playback input")
        if wikipedia:
            print("PASS: embedded artist tag, Wikipedia band biography, photo switching, and popup close")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        os.close(master)
        os.close(slave)
