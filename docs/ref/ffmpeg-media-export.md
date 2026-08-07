# ffmpeg 媒體匯出調研（media-export P1）

> 來源：本機 `ffmpeg n8.1.2`（2026-08-08 實測輸出）
> 用途：T1 FfmpegAdapter probe() / T2 optimize / T4 filmstrip 的指令鏈依據

## 1. 版本探測

`ffmpeg -version` 第一行：

```
ffmpeg version n8.1.2 Copyright (c) 2000-2026 the FFmpeg developers
```

- 版本字串 = 第三欄（`n8.1.2`），以 `ffmpeg version` 前綴開頭的行。

## 2. filter 探測（palettegen / paletteuse / hstack）

`ffmpeg -hide_banner -filters` 輸出格式（前兩欄為 flags + 名稱）：

```
 .. palettegen        V->V       Find the optimal palette for a given stream.
 .. paletteuse        VV->V      Use a palette to downsample an input video stream.
 .S hstack            N->V       Stack video inputs horizontally.
```

- flags 欄：2 字元（`.T`/`.S`/`..` 等；`.S` = 可伸縮，`.T` = 時間軸）
- **名稱 = 第二欄**；不存在時該行沒有該 filter（lenient：回 false）

## 3. encoder 探測（libwebp）

`ffmpeg -hide_banner -encoders` 相關行：

```
 V....D libwebp_anim         libwebp WebP image (codec webp)
 V....D libwebp              libwebp WebP image (codec webp)
```

- flags 欄 6 字元，`V` 開頭 = video，`D` = 直接渲染（非 decode）
- **名稱 = 第二欄**（`libwebp`/`libwebp_anim`）

## 4. GIF 雙 Pass 指令鏈（T2 optimize）

pass1 生成調色盤：

```sh
ffmpeg -y -i input.webm -vf "fps=15,scale=480:-1:flags=lanczos,palettegen" palette.png
```

pass2 套用調色盤輸出 GIF：

```sh
ffmpeg -y -i input.webm -i palette.png -lavfi "fps=15,scale=480:-1:flags=lanczos[x];[x][1:v]paletteuse" output.gif
```

## 5. 抽幀（T4 filmstrip）

指定時間點抽單幀 PNG（`-ss` 在 `-i` 前 = 快速 seek，精確度到關鍵幀；在 `-i` 後 = 精確逐幀 seek，較慢）：

```sh
ffmpeg -y -ss 3.5 -i input.webm -frames:v 1 frame_3500ms.png
```

## 6. 橫向拼接（T4 filmstrip hstack）

多張 PNG 橫向拼接：

```sh
ffmpeg -y -i f1.png -i f2.png -i f3.png -filter_complex "[0][1][2]hstack=inputs=3" strip.png
```

- 所有輸入需同高（不同尺寸先 `scale` 統一）
