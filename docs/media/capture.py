import argparse
import json
import os
import select
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MEDIA = ROOT / "docs/media"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve(strict=True)
    children = []
    with tempfile.TemporaryDirectory(prefix="bronze-set-b-") as folder:
        work = Path(folder)
        shutil.copyfile(MEDIA / "fixture-notes.json", work / "notes.json")
        (work / "preferences.json").write_text(
            json.dumps(
                {
                    "version": 1,
                    "keep_on_top": False,
                    "scope": "all",
                    "frame": {"x": 40, "y": 40, "width": 400, "height": 700},
                }
            )
        )
        read_fd, write_fd = os.pipe()
        try:
            with (work / "xvfb.log").open("wb") as log:
                display = subprocess.Popen(
                    [
                        "Xvfb",
                        "-displayfd",
                        str(write_fd),
                        "-screen",
                        "0",
                        "1440x1000x24",
                        "-nolisten",
                        "tcp",
                    ],
                    pass_fds=(write_fd,),
                    stdout=log,
                    stderr=log,
                )
            children.append(display)
            os.close(write_fd)
            if not select.select([read_fd], [], [], 10)[0]:
                raise RuntimeError("The isolated Xvfb display did not start.")
            number = os.read(read_fd, 32).decode().strip()
            if not number.isdecimal():
                raise RuntimeError("Xvfb did not return a display number.")
            env = dict(
                os.environ,
                DISPLAY=f":{number}",
                VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/lvp_icd.json",
            )
            with (work / "app.log").open("wb") as log:
                app = subprocess.Popen(
                    [str(binary), "--data", str(work)], env=env, stdout=log, stderr=log
                )
            children.append(app)
            window = subprocess.check_output(
                ["xdotool", "search", "--sync", "--onlyvisible", "--name", "^Bronze$"],
                env=env,
                text=True,
                timeout=20,
            ).splitlines()[0]

            def xdo(*values):
                subprocess.run(
                    ["xdotool", *map(str, values)], env=env, check=True, timeout=10
                )

            def grab(name):
                subprocess.run(
                    [
                        "ffmpeg",
                        "-loglevel",
                        "error",
                        "-f",
                        "x11grab",
                        "-video_size",
                        "400x700",
                        "-i",
                        f":{number}+40,40",
                        "-frames:v",
                        "1",
                        "-y",
                        str(MEDIA / name),
                    ],
                    check=True,
                    timeout=10,
                )

            def record(name, action):
                video = work / f"{name}.mkv"
                recorder = subprocess.Popen(
                    [
                        "ffmpeg",
                        "-loglevel",
                        "error",
                        "-f",
                        "x11grab",
                        "-draw_mouse",
                        "0",
                        "-framerate",
                        "10",
                        "-video_size",
                        "400x700",
                        "-i",
                        f":{number}+40,40",
                        "-t",
                        "15",
                        "-c:v",
                        "ffv1",
                        "-y",
                        str(video),
                    ],
                    stdin=subprocess.PIPE,
                )
                try:
                    time.sleep(0.8)
                    action()
                    time.sleep(1.7)
                finally:
                    recorder.communicate(b"q\n", timeout=10)
                subprocess.run(
                    [
                        "ffmpeg",
                        "-loglevel",
                        "error",
                        "-i",
                        str(video),
                        "-filter_complex",
                        "[0:v]split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4",
                        "-loop",
                        "0",
                        "-y",
                        str(MEDIA / f"{name}.gif"),
                    ],
                    check=True,
                    timeout=30,
                )

            xdo("windowfocus", window)
            xdo("mousemove", "--window", window, 370, 582, "click", 1)
            xdo("mousemove", 900, 850)
            time.sleep(0.4)
            grab("overview-linux.png")

            def search():
                xdo("mousemove", "--window", window, 120, 28, "click", 1)
                xdo("type", "--clearmodifiers", "--delay", "150", "sensor")
                xdo("mousemove", 900, 850)
                time.sleep(0.5)
                grab("search-linux.png")

            record("search", search)
            xdo("key", "ctrl+a", "BackSpace")
            xdo("mousemove", "--window", window, 370, 582, "click", 1)

            def switch():
                xdo("key", "ctrl+k")
                time.sleep(0.6)
                xdo("type", "--clearmodifiers", "--delay", "160", "Weekend")
                time.sleep(0.6)
                xdo("key", "Return")
                time.sleep(0.6)
                grab("section-linux.png")

            record("sections", switch)
            xdo("key", "ctrl+k")
            time.sleep(0.25)
            xdo("type", "--clearmodifiers", "Field")
            xdo("key", "Return")
            time.sleep(0.3)
            captured = "Turn the sensor readings into a one-page weekly summary."
            owner = subprocess.Popen(
                ["python3", str(ROOT / "scripts/own-primary.py"), captured], env=env
            )
            children.append(owner)
            time.sleep(0.5)
            xdo("windowfocus", window)

            def capture():
                xdo("key", "ctrl+alt+c")
                time.sleep(0.6)
                notes = json.loads((work / "notes.json").read_text())
                if not any(
                    note["text"] == captured
                    for section in notes["sections"]
                    for note in section["notes"]
                ):
                    raise RuntimeError(
                        "The real global capture did not save the fixture selection."
                    )
                grab("capture-linux.png")

            record("quick-capture", capture)
            print("Captured the running native app using only temporary fixture data.")
        finally:
            os.close(read_fd)
            for child in reversed(children):
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait()


if __name__ == "__main__":
    main()
