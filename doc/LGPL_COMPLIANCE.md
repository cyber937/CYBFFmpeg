# LGPL Compliance — CYBFFmpeg

This document outlines the LGPL v3.0 compliance requirements for Mac App Store distribution.

## Overview

FFmpeg is licensed under LGPL v2.1+ (or GPL if compiled with GPL components). For Mac App Store distribution, we MUST use LGPL-only build without any GPL components.

### Key Requirements

1. FFmpeg must be built as **dynamic libraries**
2. No GPL-licensed codecs (libx264, libx265, etc.)
3. Source code must be available
4. Users must be able to replace the FFmpeg libraries
5. Proper attribution in app

## License Compatibility Matrix

### LGPL-Compatible (ALLOWED)

| Component | License | Status |
|-----------|---------|--------|
| FFmpeg core | LGPL v2.1+ | OK |
| libavcodec | LGPL v2.1+ | OK |
| libavformat | LGPL v2.1+ | OK |
| libavutil | LGPL v2.1+ | OK |
| libswscale | LGPL v2.1+ | OK |
| libdav1d | BSD-2-Clause | OK |
| libvpx | BSD | OK |
| libaom | BSD-2-Clause | OK |
| VideoToolbox | Apple (System) | OK |
| AudioToolbox | Apple (System) | OK |

### GPL (PROHIBITED)

| Component | License | Status |
|-----------|---------|--------|
| libx264 | GPL v2+ | PROHIBITED |
| libx265 | GPL v2+ | PROHIBITED |
| libfdk-aac | Non-free | PROHIBITED |
| libaribb24 | LGPL but GPL deps | PROHIBITED |

## Build Requirements

### Required Configure Flags

```bash
./configure \
  --enable-shared \      # Dynamic libraries (REQUIRED)
  --disable-static \     # No static linking
  --enable-version3 \    # LGPL v3 (recommended)
  --disable-gpl \        # NO GPL components
  --disable-nonfree      # NO non-free components
```

### Prohibited Configure Flags

```bash
# NEVER USE THESE
--enable-gpl
--enable-nonfree
--enable-libx264
--enable-libx265
--enable-libfdk-aac
--enable-libopencore-amrnb
--enable-libopencore-amrwb
--enable-libvo-amrwbenc
```

## Distribution Requirements

### 1. Dynamic Linking

FFmpeg libraries MUST be:
- Dynamically linked (.dylib on macOS)
- User-replaceable
- Located in app bundle's Frameworks directory

```
MyApp.app/
└── Contents/
    └── Frameworks/
        ├── libavcodec.dylib
        ├── libavformat.dylib
        ├── libavutil.dylib
        └── libswscale.dylib
```

### 2. Source Code Availability

You MUST provide:
- Complete FFmpeg source code used in build
- All modifications made to FFmpeg
- Build scripts and configuration

Options:
- Host on GitHub (recommended)
- Include written offer in app
- Link in App Store description

### 3. Attribution

Include in app "About" section or Help:

```
This application uses FFmpeg (https://ffmpeg.org/),
licensed under the LGPL version 2.1 or later.

Source code is available at:
https://github.com/yourusername/ffmpeg-lgpl-build

FFmpeg is a trademark of Fabrice Bellard.
```

### 4. LGPL Notice

Include in app's legal notices:

```
LGPL License Notice
-------------------
This application uses FFmpeg, which is licensed under the
GNU Lesser General Public License version 2.1 or later (LGPL).

You have the right to:
- Replace the FFmpeg libraries with your own modified version
- Request the source code for the FFmpeg libraries

The FFmpeg source code and build instructions are available at:
[Your GitHub URL]

A copy of the LGPL license can be found at:
https://www.gnu.org/licenses/lgpl-2.1.html
```

## Verification Process

### Pre-Release Checklist

- [ ] FFmpeg built with `--disable-gpl --disable-nonfree`
- [ ] All libraries are dynamic (.dylib)
- [ ] No GPL codec symbols in binaries
- [ ] Source code repository public and up-to-date
- [ ] Attribution visible in app
- [ ] LGPL notice in legal section
- [ ] Libraries are user-replaceable

### Automated Verification

Run before every release:

```bash
./ffmpeg-build/scripts/verify-lgpl.sh
```

### Manual Verification

```bash
# Check library type
file Frameworks/*.dylib
# Should show: "Mach-O 64-bit dynamically linked shared library arm64"

# Check for GPL symbols
nm Frameworks/libavcodec.dylib | grep -i "x264\|x265"
# Should return nothing

# Check FFmpeg config
strings Frameworks/libavcodec.dylib | grep "configuration"
# Should NOT contain --enable-gpl
```

## Apple App Store Guidelines

### Relevant Guidelines

- **2.5.2**: Apps must be self-contained and not download executable code
  - Solution: Bundle FFmpeg in app, don't download

- **4.3**: Apps may not include copyleft code that requires app source disclosure
  - Solution: LGPL with dynamic linking is acceptable

### Previous Approvals

Apps using LGPL FFmpeg have been approved:
- VLC media player
- IINA
- Infuse
- Many others

## Source Code Repository

### Required Contents

```
ffmpeg-lgpl-build/
├── README.md           # Build instructions
├── LICENSE             # LGPL v2.1 text
├── CHANGELOG.md        # Version changes
├── ffmpeg-7.0/         # Unmodified FFmpeg source
├── patches/            # Any modifications
│   └── *.patch
├── build.sh            # Build script
└── config.txt          # Configure flags used
```

### README Template

```markdown
# FFmpeg LGPL Build for CYBFFmpeg

This repository contains the FFmpeg source code and build scripts
used in CYBFFmpeg application.

## License

FFmpeg is licensed under LGPL v2.1+

## Build Instructions

1. Install dependencies:
   ```bash
   brew install nasm yasm pkg-config dav1d libvpx aom
   ```

2. Build FFmpeg:
   ```bash
   ./build.sh
   ```

## Modifications

[List any patches applied]

## Version

FFmpeg version: 7.0
Build date: YYYY-MM-DD
```

## Handling Updates

### When FFmpeg Updates

1. Download new FFmpeg source
2. Apply any existing patches
3. Run LGPL verification
4. Update source code repository
5. Update version in attribution

### When Adding Codecs

1. Verify codec license is LGPL/BSD compatible
2. Update configure flags
3. Run LGPL verification
4. Update documentation

## Contact for Legal Questions

For legal questions about LGPL compliance:

- FFmpeg Legal: legal@ffmpeg.org
- FSF Licensing: licensing@fsf.org

## Summary Checklist

### Build Time

- [x] Use `--disable-gpl --disable-nonfree`
- [x] Use `--enable-shared --disable-static`
- [x] Only link LGPL/BSD libraries
- [x] Run verify-lgpl.sh

### Distribution Time

- [x] Bundle as dynamic libraries
- [x] Publish source code
- [x] Add attribution
- [x] Add LGPL notice
- [x] Ensure libraries are replaceable

### App Store Submission

- [x] All verification passes
- [x] Source code URL in app metadata
- [x] Legal notices complete
- [x] Test library replacement works
