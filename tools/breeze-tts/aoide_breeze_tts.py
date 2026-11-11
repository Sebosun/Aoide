#!/usr/bin/env python3
"""Breeze TTS 2 for Aoide: a small loopback server with the OpenAI speech API.

Aoide talks to it with its "OpenAI-compatible speech server" engine. Set it up once:

    git clone https://github.com/breezeblue-ai/breeze-tts.git && cd breeze-tts
    uv venv --python 3.12 --python-preference only-managed venv
    VIRTUAL_ENV=venv uv pip install -r requirements.txt
    venv/bin/hf download BreezeBlue/Breeze-TTS-2 --local-dir ../breeze-tts-2
    venv/bin/python path/to/aoide_breeze_tts.py --breeze . --model ../breeze-tts-2 --port 8883

Breeze TTS 2 weights are for research and non-commercial use only (BreezeBlue licence).

Voices are pairs of files in the voices folder (default ~/.local/share/aoide/breeze-voices):
`<name>.wav`, a clean 5-15 s recording, and `<name>.txt`, its exact transcript. Every sentence
is cloned from that clip, so a book keeps one steady voice. Two ways to add a voice:

    # Describe it; Breeze designs a reference clip once and keeps it.
    venv/bin/python aoide_breeze_tts.py --breeze . --model ../breeze-tts-2 \
        --design "Mara" "A warm woman in her forties, low and unhurried, like a late-night radio host."
    # Or copy in a recording you have the rights to, with its transcript as Name.txt.

On the first start, with an empty folder, a few narrator voices are designed for you. Designing
needs normal mode; once voices exist, `--fast` (CUDA graphs, about 11 GB) is about 2.5x quicker.

API (loopback only by default):
    GET  /v1/audio/voices -> {"voices": [{"id": "Mara", "name": "Mara"}, ...]}
    POST /v1/audio/speech {"input": "...", "voice": "Mara",
                           "instructions": "optional direction, e.g. Speak softly."}
                          -> audio/wav (mono, 16-bit, 24 kHz)
"""

import argparse
import io
import ipaddress
import json
import os
import re
import sys
import threading
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

MAX_BODY = 256 * 1024
MAX_TEXT = 4096
SEED = 42
CLONE_CFG = 1.0
DIRECT_CFG = 4.0

# Read once in the designer's voice, then used as the reference transcript for that voice.
REFERENCE_TEXT = (
    "The road ran down past the mill and along the river, where the evening light lay quiet "
    "on the water. I had walked it many times before, but never this late, and never alone."
)

STARTER_VOICES = {
    "Hazel": "A warm, clear woman in her thirties reading an audiobook: calm, intimate, "
             "evenly paced, with gentle expression.",
    "Arthur": "A mature man with a deep, resonant voice narrating a novel: steady, measured "
              "and quietly expressive.",
    "Nell": "A bright young woman with a light British accent telling a story: lively but "
            "unhurried, with crisp diction.",
}

_runtime = None
_voices_dir = None
_fast = False
_lock = threading.Lock()


def clone_only_profile(path):
    """Writes a copy of Breeze's fast warmup profile with only the graphs cloning uses.

    Cloning from a reference clip runs at CFG 1.0, one branch. The stock profile also captures
    the two-branch CFG graphs that designing a voice needs, which do not fit a 16 GB card. The
    text encoder's batch counts text segments (reference transcript plus sentence), not CFG
    branches, so its graphs stay. Returns the path of the copy.
    """
    import tempfile

    profile = json.loads(Path(path).read_text())
    profile["service"]["cfg_scales"] = [CLONE_CFG]
    stages = profile["stages"]
    for name in ("backbone_prefill", "backbone_decode"):
        stages[name]["graphs"] = [g for g in stages[name]["graphs"] if g["branch_batch_size"] == 1]
    stages["depth_decoder"]["graphs"] = [
        g for g in stages["depth_decoder"]["graphs"] if g["batch_size"] == 1
    ]
    out = Path(tempfile.mkdtemp(prefix="aoide-breeze-")) / "fast-clone.json"
    out.write_text(json.dumps(profile))
    return out


def load(breeze_dir, model_dir, fast):
    """Loads Breeze once. `breeze_dir` is a checkout of github.com/breezeblue-ai/breeze-tts."""
    global _runtime
    sys.path.insert(0, str(breeze_dir))
    from dataclasses import replace

    import breeze_infer.templates as templates
    from breeze_infer.runtime import load_runtime, resolve_device, update_generation_config_for_breeze
    from models.fast_streaming import FastBreezeStreamingRuntime, FastStreamingConfig
    from models.warmup_profile import load_warmup_profile

    # Every sentence of a voice uses the same reference clip: encode it once.
    encode, encoded = templates._encode_prompt_audio, {}

    def encode_once(tokenizer, path):
        key = (str(path), os.path.getmtime(path))
        if key not in encoded:
            encoded[key] = encode(tokenizer, path)
        return encoded[key].clone()

    templates._encode_prompt_audio = encode_once

    tokenizer, model, audio_tokenizer = load_runtime(
        Path(model_dir), device=resolve_device(), attn_implementation="eager"
    )
    update_generation_config_for_breeze(model)
    config = FastStreamingConfig(
        max_new_tokens=1500, max_seq_len=2048, fast_all=fast or None,
        fast_text_encoder=fast, fast_backbone_prefill=fast, fast_backbone_decode=fast,
        fast_depth_decoder=fast, fast_codec=fast, repetition_penalty=1.1,
    )
    streaming = FastBreezeStreamingRuntime(model, audio_tokenizer, config, tokenizer=tokenizer)
    if streaming.fast_enabled:
        profile = load_warmup_profile(clone_only_profile(Path(breeze_dir) / "configs" / "fast.json"))
        streaming.warmup_from_profile(replace(profile, codec_chunk_frames=streaming.codec_chunk_frames))
    _runtime = {
        "tokenizer": tokenizer, "model": model, "audio_tokenizer": audio_tokenizer,
        "streaming": streaming, "templates": templates,
    }


def generate(text, reference=None, instruction=None):
    """Returns (float32 samples, sample rate) for one request."""
    import numpy as np
    from breeze_infer.runtime import set_all_seeds

    templates = _runtime["templates"]
    request = {"id": "aoide", "text": text, "speaker": "S0"}
    if instruction:
        request["instruction"] = instruction
    if reference:
        request["ref_audio_path"], request["ref_text"] = reference
    cfg = DIRECT_CFG if instruction else CLONE_CFG
    with _lock:
        set_all_seeds(SEED)
        inputs = templates.prepare_inputs(
            _runtime["tokenizer"], _runtime["audio_tokenizer"], _runtime["model"], [request],
            templates.get_template(templates.select_template_name(request)),
            guidance_scale=cfg, guidance_scale_ref=None, guidance_scale_ins=None,
        )
        chunks = [c.audio for c in _runtime["streaming"].iter_audio_chunks(inputs, request_id="aoide", seed=SEED)]
    audio = np.concatenate(chunks) if chunks else np.zeros(0, dtype="float32")
    return audio, _runtime["streaming"].sample_rate


def to_wav(samples, rate):
    pcm = (samples.clip(-1.0, 1.0) * 32767.0).astype("<i2").tobytes()
    out = io.BytesIO()
    with wave.open(out, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(pcm)
    return out.getvalue()


def voice_name(name):
    name = re.sub(r"[^\w\- ]", "", name).strip()
    if not name:
        raise ValueError("a voice name needs letters or digits")
    return name


def design(name, description):
    """Designs a voice from a description and keeps its reference clip in the voices folder."""
    name = voice_name(name)
    samples, rate = generate(REFERENCE_TEXT, instruction=description)
    _voices_dir.mkdir(parents=True, exist_ok=True)
    (_voices_dir / f"{name}.wav").write_bytes(to_wav(samples, rate))
    (_voices_dir / f"{name}.txt").write_text(REFERENCE_TEXT + "\n")
    (_voices_dir / f"{name}.description.txt").write_text(description + "\n")
    print(f"designed voice {name} ({len(samples) / rate:.1f} s reference)", flush=True)


def voices():
    """Voices with both a clip and a transcript, by name."""
    found = {}
    for wav in sorted(_voices_dir.glob("*.wav")):
        transcript = wav.with_suffix(".txt")
        if transcript.is_file() and transcript.read_text().strip():
            found[wav.stem] = (str(wav), transcript.read_text().strip())
    return found


class Handler(BaseHTTPRequestHandler):
    server_version = "aoide-breeze-tts"

    def log_message(self, format, *args):
        pass

    def reply(self, status, body, content_type="application/json"):
        if isinstance(body, (dict, list)):
            body = json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def error(self, status, message):
        self.reply(status, {"error": {"message": message}})

    def do_GET(self):
        if self.path in ("/v1/audio/voices", "/v1/voices"):
            self.reply(200, {"voices": [{"id": n, "name": n} for n in voices()]})
        elif self.path in ("/health", "/v1/models"):
            self.reply(200, {"ready": True, "voices": len(voices())})
        else:
            self.error(404, "not found")

    def do_POST(self):
        if self.path != "/v1/audio/speech":
            return self.error(404, "not found")
        length = int(self.headers.get("Content-Length") or 0)
        if length <= 0 or length > MAX_BODY:
            return self.error(413, "request body missing or too large")
        try:
            body = json.loads(self.rfile.read(length))
        except ValueError:
            return self.error(400, "expected JSON")
        text = str(body.get("input") or "").strip()
        if not text or len(text) > MAX_TEXT:
            return self.error(400, "input is empty or too long")
        reference = voices().get(str(body.get("voice") or ""))
        if reference is None:
            return self.error(400, f"unknown voice {body.get('voice')!r}")
        if body.get("response_format", "wav") != "wav":
            return self.error(400, "only response_format wav is supported")
        instruction = str(body.get("instructions") or "").strip() or None
        if instruction and _fast:
            return self.error(400, "instructions need the server started without --fast")
        try:
            samples, rate = generate(text, reference, instruction)
        except Exception as exc:  # Report model failures to Aoide instead of dropping the connection.
            return self.error(500, f"generation failed: {exc}")
        self.reply(200, to_wav(samples, rate), "audio/wav")


def main():
    global _voices_dir, _fast
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--breeze", required=True, help="checkout of breezeblue-ai/breeze-tts")
    parser.add_argument("--model", required=True, help="Breeze TTS 2 checkpoint folder")
    parser.add_argument("--voices", default=str(Path.home() / ".local/share/aoide/breeze-voices"))
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8883)
    parser.add_argument("--fast", action="store_true",
                        help="CUDA-graph fast path: quicker, more GPU memory, slower start; "
                             "design voices without it")
    parser.add_argument("--design", nargs=2, metavar=("NAME", "DESCRIPTION"),
                        help="design a voice from a description, save it, and exit")
    parser.add_argument("--allow-lan", action="store_true",
                        help="allow binding to a non-loopback address")
    args = parser.parse_args()
    host = "127.0.0.1" if args.host == "localhost" else args.host
    if not ipaddress.ip_address(host).is_loopback and not args.allow_lan:
        sys.exit("refusing to listen on a non-loopback address without --allow-lan")
    _voices_dir = Path(args.voices).expanduser()
    _fast = args.fast
    if _fast and (args.design or not voices()):
        sys.exit("designing voices needs the server started once without --fast")
    load(args.breeze, args.model, args.fast)
    if args.design:
        design(*args.design)
        return
    if not voices():
        for name, description in STARTER_VOICES.items():
            design(name, description)
    generate("Ready.", next(iter(voices().values())))  # Warm up before taking requests.
    print(f"aoide-breeze-tts ready on http://{host}:{args.port} ({len(voices())} voices)", flush=True)
    ThreadingHTTPServer((host, args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
