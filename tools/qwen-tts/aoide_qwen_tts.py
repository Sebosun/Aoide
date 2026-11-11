#!/usr/bin/env python3
"""Qwen3-TTS for Aoide: a small loopback server with the OpenAI speech API.

Aoide talks to it with its "OpenAI-compatible speech server" engine. Set it up once:

    uv venv --python 3.12 --python-preference only-managed venv
    VIRTUAL_ENV=venv uv pip install faster-qwen3-tts "transformers==5.15.1"
    venv/bin/python tools/qwen-tts/aoide_qwen_tts.py --port 8882

The first start downloads the model (about 4.5 GB, into the Hugging Face cache) and spends
about a minute preparing CUDA graphs. Generation runs at roughly 3-4x real time on a recent
NVIDIA GPU and needs about 4.5 GB of video memory for the 1.7B model.

faster-qwen3-tts (CUDA graphs) is used because the reference `qwen-tts` package runs at about
1.3x real time on the same GPU. transformers is pinned because 5.22 breaks the speech codec.

API (loopback only by default):
    GET  /v1/audio/voices -> {"voices": [{"id": "ryan", "name": "Ryan (English)"}, ...]}
    POST /v1/audio/speech {"input": "...", "voice": "ryan", "language": "en",
                           "instructions": "optional speaking style"}
                          -> audio/wav (mono, 16-bit, 24 kHz)
"""

import argparse
import io
import ipaddress
import json
import sys
import threading
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MAX_BODY = 256 * 1024
MAX_TEXT = 64 * 1024

# The CustomVoice model's built-in speakers and the language each sounds most natural in.
SPEAKERS = {
    "ryan": "Ryan (English)",
    "aiden": "Aiden (English)",
    "vivian": "Vivian (Chinese)",
    "serena": "Serena (Chinese)",
    "uncle_fu": "Uncle Fu (Chinese)",
    "dylan": "Dylan (Beijing Chinese)",
    "eric": "Eric (Sichuan Chinese)",
    "ono_anna": "Ono Anna (Japanese)",
    "sohee": "Sohee (Korean)",
}

# Book languages arrive as BCP 47 codes ("en", "en-GB"); the model wants names.
LANGUAGES = {
    "zh": "chinese", "en": "english", "ja": "japanese", "ko": "korean", "de": "german",
    "fr": "french", "ru": "russian", "pt": "portuguese", "es": "spanish", "it": "italian",
}

_model = None
_voices = []
_default_instruct = None
_lock = threading.Lock()


def load(model_name):
    global _model, _voices
    from faster_qwen3_tts import FasterQwen3TTS

    _model = FasterQwen3TTS.from_pretrained(model_name)
    supported = _model.model.get_supported_speakers() or []
    _voices = [{"id": s, "name": SPEAKERS.get(s, s.replace("_", " ").title())} for s in supported]
    # Capture the CUDA graphs now rather than on the first sentence.
    speak("Ready.", _voices[0]["id"] if _voices else "ryan", "english", None)


def language_name(code):
    if not code:
        return "auto"
    return LANGUAGES.get(str(code).lower().replace("_", "-").split("-")[0], "auto")


def speak(text, voice, language, instruct):
    with _lock:
        wavs, rate = _model.generate_custom_voice(
            text=text, speaker=voice, language=language, instruct=instruct
        )
    samples = wavs[0]
    if hasattr(samples, "detach"):
        samples = samples.detach().float().cpu().numpy()
    pcm = (samples.clip(-1.0, 1.0) * 32767.0).astype("<i2").tobytes()
    out = io.BytesIO()
    with wave.open(out, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(rate)
        wav.writeframes(pcm)
    return out.getvalue()


class Handler(BaseHTTPRequestHandler):
    server_version = "aoide-qwen-tts"

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
            self.reply(200, {"voices": _voices})
        elif self.path in ("/health", "/v1/models"):
            self.reply(200, {"ready": True, "voices": len(_voices)})
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
        voice = str(body.get("voice") or "").strip().lower()
        if not text or len(text.encode()) > MAX_TEXT:
            return self.error(400, "input is empty or too long")
        if voice not in {v["id"] for v in _voices}:
            return self.error(400, f"unknown voice {voice!r}")
        if body.get("response_format", "wav") != "wav":
            return self.error(400, "only response_format wav is supported")
        instruct = str(body.get("instructions") or "").strip() or _default_instruct
        try:
            audio = speak(text, voice, language_name(body.get("language")), instruct)
        except Exception as exc:  # Report model failures to Aoide instead of dropping the connection.
            return self.error(500, f"generation failed: {exc}")
        self.reply(200, audio, "audio/wav")


def main():
    global _default_instruct
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8882)
    parser.add_argument("--model", default="Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice")
    parser.add_argument("--instruct", default=None,
                        help='default speaking style, e.g. "Calm, warm audiobook narration."')
    parser.add_argument("--allow-lan", action="store_true",
                        help="allow binding to a non-loopback address")
    args = parser.parse_args()
    host = "127.0.0.1" if args.host == "localhost" else args.host
    if not ipaddress.ip_address(host).is_loopback and not args.allow_lan:
        sys.exit("refusing to listen on a non-loopback address without --allow-lan")
    _default_instruct = args.instruct
    load(args.model)
    print(f"aoide-qwen-tts ready on http://{host}:{args.port} ({len(_voices)} voices)", flush=True)
    ThreadingHTTPServer((host, args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
