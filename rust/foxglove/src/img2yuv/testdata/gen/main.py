from argparse import ArgumentParser
from pathlib import Path

import numpy as np
from PIL import Image

RGB = [
    (0, 0, 0),
    (64, 64, 64),
    (128, 128, 128),
    (192, 192, 192),
    (255, 255, 255),
    (255, 0, 0),
    (0, 255, 0),
    (0, 0, 255),
    (255, 255, 0),
    (0, 255, 255),
    (255, 0, 255),
    (255, 128, 0),
    (128, 0, 255),
    (0, 128, 255),
    (128, 255, 0),
    (255, 0, 128),
]

# These constants should be kept in sync with src/tests.rs.
W_BAR = 4
W = len(RGB) * W_BAR
H = 16
PAD = 5


def generate_image():
    rgb = np.zeros((H, W, 3), dtype=np.uint8)
    for i, (r, g, b) in enumerate(RGB):
        rgb[:, i * W_BAR : (i + 1) * W_BAR] = (r, g, b)
    alpha = np.linspace(255, 0, H, dtype=np.uint8)[:, None]
    alpha_channel = np.repeat(alpha, W, axis=1)
    rgba = np.dstack((rgb, alpha_channel))
    return rgb, rgba


def rgb_to_yuv(rgb):
    # Normalize
    r = rgb[..., 0].astype(np.float32) / 255.0
    g = rgb[..., 1].astype(np.float32) / 255.0
    b = rgb[..., 2].astype(np.float32) / 255.0

    # BT.709 luma (Kr=0.2126, Kb=0.0722)
    y = 0.2126 * r + 0.7152 * g + 0.0722 * b

    # Chroma using standard 709 matrix (y, cb, cr in [-0.5..0.5] for cb/cr)
    cb = -0.114572 * r - 0.385428 * g + 0.5 * b
    cr = 0.5 * r - 0.454153 * g - 0.045847 * b

    # Map to 8-bit limited range
    y8 = np.round(16.0 + 219.0 * y)
    cb8 = np.round(128.0 + 224.0 * cb)
    cr8 = np.round(128.0 + 224.0 * cr)

    # Clip to legal ranges
    y8 = np.clip(y8, 16, 235).astype(np.uint8)
    cb8 = np.clip(cb8, 16, 240).astype(np.uint8)
    cr8 = np.clip(cr8, 16, 240).astype(np.uint8)

    # Stack back as Y, Cb, Cr
    return (y8, cb8, cr8)


def write_padded(dir: Path, name: str, img):
    # Reshape to two dimensions and reinterpret as u8.
    twodim = np.ascontiguousarray(img).reshape(H, -1).view(np.uint8)
    # Add columns of padding.
    img_pad = np.pad(
        twodim,
        pad_width=((0, 0), (0, PAD)),
        mode="constant",
        constant_values=0,
    )
    img_pad.tofile(dir / name)


def write_raw_formats(dir: Path, stem: str, rgb, rgba):
    rgb.tofile(dir / f"{stem}.rgb8.raw")
    write_padded(dir, f"{stem}.rgb8.pad.raw", rgb)

    bgr = rgb[:, :, ::-1]
    bgr.tofile(dir / f"{stem}.bgr8.raw")
    write_padded(dir, f"{stem}.bgr8.pad.raw", bgr)

    rgba.tofile(dir / f"{stem}.rgba8.raw")
    write_padded(dir, f"{stem}.rgba8.pad.raw", rgba)

    bgra = rgba[..., [2, 1, 0, 3]]
    bgra.tofile(dir / f"{stem}.bgra8.raw")
    write_padded(dir, f"{stem}.bgra8.pad.raw", bgra)

    y, u, v = rgb_to_yuv(rgb)
    yuyv = np.zeros((H, W * 2), dtype=np.uint8)
    uyvy = np.zeros((H, W * 2), dtype=np.uint8)

    for i in range(0, W, 2):
        yuyv[:, i * 2] = y[:, i]
        yuyv[:, i * 2 + 1] = u[:, i]
        yuyv[:, i * 2 + 2] = y[:, i + 1]
        yuyv[:, i * 2 + 3] = v[:, i]

        uyvy[:, i * 2] = u[:, i]
        uyvy[:, i * 2 + 1] = y[:, i]
        uyvy[:, i * 2 + 2] = v[:, i]
        uyvy[:, i * 2 + 3] = y[:, i + 1]

    yuyv.tofile(dir / f"{stem}.yuyv.raw")
    uyvy.tofile(dir / f"{stem}.uyvy.raw")
    write_padded(dir, f"{stem}.yuyv.pad.raw", yuyv)
    write_padded(dir, f"{stem}.uyvy.pad.raw", uyvy)

    # Rescale Y from the limited [16, 235] range to [0.0, 1.0].
    mono32 = (y.astype(np.float32) - 16.0) / 219.0
    mono32fbe = mono32.astype(">f4")
    mono32fbe.tofile(dir / f"{stem}.mono32fbe.raw")
    mono32fle = mono32.astype("<f4")
    mono32fle.tofile(dir / f"{stem}.mono32fle.raw")
    write_padded(dir, f"{stem}.mono32fbe.pad.raw", mono32fbe)
    write_padded(dir, f"{stem}.mono32fle.pad.raw", mono32fle)

    mono16 = (mono32 * 65535.0).astype(np.uint16)
    mono16be = mono16.astype(">u2")
    mono16be.tofile(dir / f"{stem}.mono16be.raw")
    mono16le = mono16.astype("<u2")
    mono16le.tofile(dir / f"{stem}.mono16le.raw")
    write_padded(dir, f"{stem}.mono16be.pad.raw", mono16be)
    write_padded(dir, f"{stem}.mono16le.pad.raw", mono16le)

    mono8 = (mono32 * 255.0).astype(np.uint8)
    mono8.tofile(dir / f"{stem}.mono8.raw")
    write_padded(dir, f"{stem}.mono8.pad.raw", mono8)


def write_bayer_formats(dir: Path, stem: str, rgb):
    # Losslessly decimate by 2, since bayer is composed of 2x2 tiles.
    assert H % 2 == 0
    assert W % 2 == 0
    assert W_BAR % 2 == 0
    rgb_ds = rgb[::2, ::2]

    R, G, B = rgb_ds[..., 0], rgb_ds[..., 1], rgb_ds[..., 2]
    cfa_channels = {
        "rggb": (R, G, G, B),
        "bggr": (B, G, G, R),
        "grbg": (G, R, B, G),
        "gbrg": (G, B, R, G),
    }
    for cfa, ch in cfa_channels.items():
        bayer = np.zeros((H, W), dtype=np.uint8)
        bayer[0::2, 0::2] = ch[0]
        bayer[0::2, 1::2] = ch[1]
        bayer[1::2, 0::2] = ch[2]
        bayer[1::2, 1::2] = ch[3]
        bayer.tofile(dir / f"{stem}.bayer8-{cfa}.raw")
        write_padded(dir, f"{stem}.bayer8-{cfa}.pad.raw", bayer)


def write_compressed_formats(dir: Path, stem: str, rgb, rgba):
    rgb = Image.fromarray(rgb)
    rgb.save(dir / f"{stem}.jpg", "JPEG")
    rgb.save(dir / f"{stem}.webp", "WEBP")
    rgba = Image.fromarray(rgba)
    rgba.save(dir / f"{stem}.png", "PNG")


def main():
    ap = ArgumentParser()
    ap.add_argument("--dir", default=Path(__file__).parent.parent)
    ap.add_argument("--stem", default="test")
    args = ap.parse_args()
    rgb, rgba = generate_image()
    write_raw_formats(args.dir, args.stem, rgb, rgba)
    write_bayer_formats(args.dir, args.stem, rgb)
    write_compressed_formats(args.dir, args.stem, rgb, rgba)


if __name__ == "__main__":
    main()
