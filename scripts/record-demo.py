#!/usr/bin/env python3
"""Record real command output as asciicast, plain text, and a standalone replay page."""
import argparse
import codecs
import datetime
import html
import json
import os
from pathlib import Path
import re
import selectors
import shlex
import signal
import subprocess
import time

TOKEN = re.compile(r"h2a_[a-f0-9]{32}_[A-Za-z0-9_-]{43}")

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, help="artifact path stem")
    parser.add_argument("--title", default="h2a live demo")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("provide a command after --")
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    header = {"version": 2, "width": 100, "height": 30, "timestamp": int(time.time()), "title": args.title, "env": {"TERM": "xterm-256color"}}
    events = [[0.0, "o", "$ " + shlex.join(command) + "\r\n"]]
    started = time.monotonic()
    proc = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, start_new_session=True)
    decoder = codecs.getincrementaldecoder("utf-8")(errors="replace")
    try:
        selector = selectors.DefaultSelector()
        selector.register(proc.stdout, selectors.EVENT_READ)
        while selector.get_map():
            for key, _ in selector.select(timeout=1):
                data = os.read(key.fileobj.fileno(), 65536)
                if not data:
                    selector.unregister(key.fileobj)
                    break
                text = decoder.decode(data)
                if text:
                    events.append([round(time.monotonic() - started, 3), "o", text.replace("\n", "\r\n")])
        proc.wait()
    except BaseException:
        os.killpg(proc.pid, signal.SIGTERM)
        proc.wait(timeout=15)
        raise
    finally:
        proc.stdout.close()
    # Redact across chunk boundaries before saving or displaying anything.
    joined = "".join(event[2] for event in events)
    for match in reversed(list(TOKEN.finditer(joined))):
        cursor = 0
        for event in events:
            start, end = cursor, cursor + len(event[2]); cursor = end
            if start < match.end() and end > match.start():
                left, right = max(0, match.start() - start), min(end - start, match.end() - start)
                event[2] = event[2][:left] + "*" * (right - left) + event[2][right:]
    transcript = "".join(event[2] for event in events).replace("\r\n", "\n")
    output.with_suffix(".txt").write_text(transcript)
    with output.with_suffix(".cast").open("w") as f:
        f.write(json.dumps(header) + "\n")
        for event in events: f.write(json.dumps(event, ensure_ascii=False) + "\n")
    payload = json.dumps(events, ensure_ascii=False).replace("<", "\\u003c")
    duration = max(events[-1][0], 0.1)
    title = html.escape(args.title)
    page = '''<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>__TITLE__</title><style>
*{box-sizing:border-box}body{margin:0;padding:32px;font:15px/1.5 system-ui;color:#e9eef5;background:#0c111b}main{max-width:1000px;margin:auto}h1{font-size:28px;letter-spacing:-.6px;margin:0 0 6px}p{color:#9aabc1;margin:0 0 24px}.terminal{background:#121b29;border:1px solid #2c394d;border-radius:16px;overflow:hidden}.bar{padding:14px 20px;border-bottom:1px solid #2c394d;color:#a9b6c8;display:flex;align-items:center;gap:8px}.dot{width:10px;height:10px;border-radius:50%;background:#4a637e}.bar span:last-child{margin-left:10px}pre{white-space:pre-wrap;word-break:break-word;font:15px/1.65 ui-monospace,SFMono-Regular,Consolas,monospace;padding:24px;margin:0;min-height:410px;color:#d8e5f6}.controls{display:flex;gap:14px;align-items:center;margin:18px 0;flex-wrap:wrap}button,select{background:#1c2b40;border:1px solid #395371;color:#e9eef5;padding:10px 16px;border-radius:8px;cursor:pointer}input{flex:1;min-width:100px;accent-color:#65d8b0}small{color:#9aabc1}#time{font-variant-numeric:tabular-nums;min-width:80px}@media(max-width:500px){body{padding:16px}pre{padding:16px;font-size:12px;min-height:400px}}
</style></head><body><main><h1>__TITLE__</h1><p>Recorded command output. The terminal states whether this is a live provider call or an offline fixture. Secrets are hidden.</p><div class="terminal"><div class="bar"><i class="dot"></i><i class="dot"></i><i class="dot"></i><span>harness-to-api / terminal</span></div><pre id="terminal" aria-live="off"></pre></div><div class="controls"><button id="play">Play</button><button id="restart">Restart</button><input id="seek" aria-label="Playback position" type="range" min="0" max="__DURATION__" step="0.01"><span id="time"></span><select id="speed" aria-label="Playback speed"><option value="1">1×</option><option value="2">2×</option><option value="4">4×</option></select></div><small>Timing reflects this recording, not a performance benchmark. __RECORDED__ · exit code __EXIT__</small></main><script>
const events=__EVENTS__,duration=__DURATION__;const terminal=document.getElementById('terminal'),seek=document.getElementById('seek'),play=document.getElementById('play');let position=duration,playing=false,last=performance.now();
function render(){terminal.textContent=events.filter(e=>e[0]<=position).map(e=>e[2]).join('').replace(/\\r/g,'');seek.value=position;document.getElementById('time').textContent=position.toFixed(1)+' / '+duration.toFixed(1)+'s';play.textContent=playing?'Pause':'Play';}
play.onclick=()=>{if(position>=duration)position=0;playing=!playing;last=performance.now();render()};document.getElementById('restart').onclick=()=>{position=0;playing=true;last=performance.now();render()};seek.oninput=()=>{position=Number(seek.value);playing=false;render()};function tick(now){if(playing){position=Math.min(duration,position+(now-last)/1000*Number(document.getElementById('speed').value));if(position>=duration)playing=false;render()}last=now;requestAnimationFrame(tick)}render();requestAnimationFrame(tick);
</script></body></html>'''
    page = page.replace("__TITLE__", title).replace("__DURATION__", str(duration)).replace("__EVENTS__", payload).replace("__EXIT__", str(proc.returncode)).replace("__RECORDED__", datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d UTC"))
    output.with_suffix(".html").write_text(page)
    print(transcript)
    print("Saved .cast, .txt, and .html under", output)
    return proc.returncode

if __name__ == "__main__":
    raise SystemExit(main())
