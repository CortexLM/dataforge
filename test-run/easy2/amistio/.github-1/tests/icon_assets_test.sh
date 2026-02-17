#!/bin/sh
set -eu

repo_root="/repo"
icon_dir="$repo_root/icons"

if [ ! -d "$icon_dir" ]; then
  echo "Missing icons directory" >&2
  exit 1
fi

png_files="amistio-1024.png amistio-128.png amistio-192.png amistio-256.png amistio-512.png apple-touch-icon.png favicon-16x16.png favicon-32x32.png"

for name in $png_files; do
  file="$icon_dir/$name"
  if [ ! -f "$file" ]; then
    echo "Missing icon $file" >&2
    exit 1
  fi
  signature=$(od -An -t x1 -N 8 "$file" | tr -d ' \n')
  if [ "$signature" != "89504e470d0a1a0a" ]; then
    echo "Expected PNG signature for $file, got $signature" >&2
    exit 1
  fi
  size=$(wc -c < "$file" | tr -d ' ')
  if [ "$size" -le 100 ]; then
    echo "Unexpectedly small PNG $file ($size bytes)" >&2
    exit 1
  fi
  ihdr=$(od -An -t x1 -j 12 -N 4 "$file" | tr -d ' \n')
  if [ "$ihdr" != "49484452" ]; then
    echo "Expected IHDR chunk in $file, got $ihdr" >&2
    exit 1
  fi
  width_bytes=$(od -An -t u1 -j 16 -N 4 "$file")
  set -- $width_bytes
  width=$(( $1 * 16777216 + $2 * 65536 + $3 * 256 + $4 ))
  height_bytes=$(od -An -t u1 -j 20 -N 4 "$file")
  set -- $height_bytes
  height=$(( $1 * 16777216 + $2 * 65536 + $3 * 256 + $4 ))
  if [ "$width" -le 0 ] || [ "$height" -le 0 ]; then
    echo "Invalid PNG dimensions for $file ($width x $height)" >&2
    exit 1
  fi
  if [ "$width" -ne "$height" ]; then
    echo "Expected square PNG for $file, got $width x $height" >&2
    exit 1
  fi
  if [ "$width" -lt 16 ] || [ "$width" -gt 4096 ]; then
    echo "PNG width out of range for $file: $width" >&2
    exit 1
  fi
  if [ "$height" -lt 16 ] || [ "$height" -gt 4096 ]; then
    echo "PNG height out of range for $file: $height" >&2
    exit 1
  fi
  if [ "$width" -eq 0 ] || [ "$height" -eq 0 ]; then
    echo "PNG dimension zero for $file" >&2
    exit 1
  fi
  if [ "$width" -ne "$height" ]; then
    echo "PNG dimensions mismatch for $file: $width x $height" >&2
    exit 1
  fi
  :
 done

ico="$icon_dir/favicon.ico"
if [ ! -f "$ico" ]; then
  echo "Missing icon $ico" >&2
  exit 1
fi

ico_signature=$(od -An -t x1 -N 4 "$ico" | tr -d ' \n')
if [ "$ico_signature" != "00000100" ]; then
  echo "Expected ICO signature for $ico, got $ico_signature" >&2
  exit 1
fi

ico_size=$(wc -c < "$ico" | tr -d ' ')
if [ "$ico_size" -le 50 ]; then
  echo "Unexpectedly small ICO $ico ($ico_size bytes)" >&2
  exit 1
fi

image_count=$(od -An -t u2 -j 4 -N 2 "$ico" | tr -d ' ')
if [ -z "$image_count" ] || [ "$image_count" -le 0 ]; then
  echo "ICO image count missing for $ico" >&2
  exit 1
fi
