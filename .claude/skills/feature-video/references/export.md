# Export

Render from the video project. Every output gets loudness-normalised to −16 LUFS, −1.5 dBTP; the bundled `npx remotion ffmpeg` has `loudnorm` and `scale` but not `fps`, `drawtext` or `select`.

## 1080p (iterate with this)

About five minutes for 32s.

```sh
npx remotion render Video out/<name>-raw.mp4 --crf=16
npx remotion ffmpeg -y -i out/<name>-raw.mp4 -c:v copy -af loudnorm=I=-16:TP=-1.5:LRA=11 -c:a aac -b:a 256k -ar 48000 out/<name>.mp4
```

## 4K

`--scale=2` renders 3840×2160 from the same DOM, so text stays sharp. It takes several times as long as 1080p and loads the machine; ask before starting one, and run it in the background.

```sh
npx remotion render Video out/<name>-4k-raw.mp4 --scale=2 --crf=16
```

Then apply the same loudnorm step to the 4K file.

## X (Twitter)

Encode the 1080p master to H.264 High, yuv420p, 60fps, about 12 Mbps, AAC-LC 48kHz stereo, with faststart. That's within X's limits (1920×1200, 60fps, 25 Mbps, 512 MB / 140s free). Don't shrink it further: X re-encodes every upload, and a pre-compressed file loses dark gradients and small UI text twice.

```sh
npx remotion ffmpeg -y -i out/<name>.mp4 -c:v libx264 -profile:v high -level 4.2 -pix_fmt yuv420p -preset slow \
  -b:v 12M -maxrate 16M -bufsize 24M -g 120 -r 60 -c:a aac -b:a 192k -ar 48000 -ac 2 -movflags +faststart out/<name>-x.mp4
```

Confirm with `npx remotion ffprobe -v error -show_entries stream=codec_name,profile,width,height,r_frame_rate -show_entries format=duration,size out/<name>-x.mp4`.
