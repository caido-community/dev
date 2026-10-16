# README image fixtures

These checked-in PNGs are solid red RGB images with eight bits per channel:

- `red16x16.png`: small valid image for README parsing and filesystem tests.
- `red256x256.png`: representative input whose lossy WebP output must be smaller
  than the source PNG and fit within the 100 KiB per-image limit.
- `red3200x2400.png`: resizing within 1600 × 1600 pixels before inlining.

GIF fixtures cover animation preservation and the dimension limit:

- `animated.gif`: two 16 × 16 frames, preserved byte-for-byte when inlined.
- `large.gif`: 1601 × 1 pixels, converted to first-frame WebP.

Tests copy these fixed source files into temporary project directories. They
verify README packaging and output size, without image generators or test-only
image-processing dependencies.
