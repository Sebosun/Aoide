# Audio fixtures

Tiny synthetic files (sine tones), made with ffmpeg as below. Exact-sample tests build WAV files
in code instead.

```sh
ffmpeg -f lavfi -i "sine=frequency=330:sample_rate=22050:duration=1" -ac 1 -c:a libmp3lame -b:a 32k \
  -metadata title="Part One" -metadata album="The Lantern Keeper" -metadata artist="A. Writer" \
  -metadata track=1 set/b-first.mp3
ffmpeg -f lavfi -i "sine=frequency=660:sample_rate=22050:duration=0.5" -ac 1 -c:a libmp3lame -b:a 32k \
  -metadata title="Part Two" -metadata album="The Lantern Keeper" -metadata artist="A. Writer" \
  -metadata track=2 set/a-second.mp3

# chapters.txt:
# ;FFMETADATA1
# title=The Lantern Keeper
# artist=A. Writer
# [CHAPTER]
# TIMEBASE=1/1000
# START=0
# END=1000
# title=Opening
# [CHAPTER]
# TIMEBASE=1/1000
# START=1000
# END=2000
# title=Closing
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=2" -i chapters.txt \
  -map 0 -map_metadata 1 -map_chapters 1 -ac 1 -c:a aac -b:a 48k -f mp4 book.m4b

ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=0.75" -ac 1 -c:a flac tone.flac
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=0.5" -ac 2 -c:a libvorbis -q:a 0 tone.ogg
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=0.5" -ac 1 -c:a libopus -b:a 16k tone.opus

# A chapter track that ends long before the audio (symphonia cannot seek past 5 s):
# short-chapters.txt holds two chapters, 0–2000 and 2000–5000 ms.
ffmpeg -f lavfi -i "sine=frequency=440:sample_rate=8000:duration=30" -i short-chapters.txt \
  -map 0 -map_metadata 1 -map_chapters 1 -ac 1 -c:a aac -b:a 8k -f mp4 short-chapters.m4b
```
