"""
Accurate PNG Unfilter and Shannon Entropy Calculator
ASD-STE100 Simplified Technical English
/ponytail /caveman
"""
import struct
import zlib
import math
import sys
import os

def paeth_predictor(a, b, c):
    p = a + b - c
    pa = abs(p - a)
    pb = abs(p - b)
    pc = abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    elif pb <= pc:
        return b
    else:
        return c

def decode_png_pixels(filepath):
    with open(filepath, 'rb') as f:
        data = f.read()

    assert data[:8] == b'\x89PNG\r\n\x1a\n', "Invalid PNG magic"
    
    pos = 8
    width, height, bit_depth, color_type = 0, 0, 0, 0
    idat_chunks = []
    
    while pos < len(data):
        length = struct.unpack('>I', data[pos:pos+4])[0]
        ctype = data[pos+4:pos+8]
        cdata = data[pos+8:pos+8+length]
        pos += 12 + length
        
        if ctype == b'IHDR':
            width, height, bit_depth, color_type = struct.unpack('>IIBB', cdata[:10])
        elif ctype == b'IDAT':
            idat_chunks.append(cdata)
        elif ctype == b'IEND':
            break
            
    decompressed = zlib.decompress(b''.join(idat_chunks))
    
    # Bytes per pixel
    bpp = 4 if color_type == 6 else (3 if color_type == 2 else 1)
    stride = 1 + width * bpp
    
    raw_pixels = bytearray(width * height * 4)
    prev_row = bytearray(width * bpp)
    
    for y in range(height):
        row_start = y * stride
        filter_type = decompressed[row_start]
        row_filtered = decompressed[row_start+1 : row_start+stride]
        row_unfiltered = bytearray(width * bpp)
        
        for x in range(width * bpp):
            filt = row_filtered[x]
            left = row_unfiltered[x - bpp] if x >= bpp else 0
            above = prev_row[x]
            upper_left = prev_row[x - bpp] if x >= bpp else 0
            
            if filter_type == 0: # None
                val = filt
            elif filter_type == 1: # Sub
                val = (filt + left) & 0xFF
            elif filter_type == 2: # Up
                val = (filt + above) & 0xFF
            elif filter_type == 3: # Average
                val = (filt + ((left + above) >> 1)) & 0xFF
            elif filter_type == 4: # Paeth
                val = (filt + paeth_predictor(left, above, upper_left)) & 0xFF
            else:
                val = filt
            row_unfiltered[x] = val
            
        prev_row = row_unfiltered
        
        # Convert to RGBA
        for px in range(width):
            out_idx = (y * width + px) * 4
            in_idx = px * bpp
            if bpp == 4: # RGBA
                raw_pixels[out_idx : out_idx+4] = row_unfiltered[in_idx : in_idx+4]
            elif bpp == 3: # RGB
                raw_pixels[out_idx] = row_unfiltered[in_idx]
                raw_pixels[out_idx+1] = row_unfiltered[in_idx+1]
                raw_pixels[out_idx+2] = row_unfiltered[in_idx+2]
                raw_pixels[out_idx+3] = 255
            elif bpp == 1: # Grayscale
                g = row_unfiltered[in_idx]
                raw_pixels[out_idx] = g
                raw_pixels[out_idx+1] = g
                raw_pixels[out_idx+2] = g
                raw_pixels[out_idx+3] = 255
                
    return width, height, raw_pixels

def calculate_shannon_entropy(pixels_rgba):
    total_pixels = len(pixels_rgba) // 4
    freq = {}
    non_zero = 0
    for i in range(0, len(pixels_rgba), 4):
        r, g, b, a = pixels_rgba[i], pixels_rgba[i+1], pixels_rgba[i+2], pixels_rgba[i+3]
        if r != 0 or g != 0 or b != 0 or a != 0:
            non_zero += 1
        color = (r << 24) | (g << 16) | (b << 8) | a
        freq[color] = freq.get(color, 0) + 1
        
    entropy = 0.0
    for count in freq.values():
        p = count / total_pixels
        if p > 0:
            entropy -= p * math.log2(p)
            
    return {
        "entropy": entropy,
        "unique_colors": len(freq),
        "total_pixels": total_pixels,
        "non_zero_pixels": non_zero,
        "non_zero_ratio": non_zero / total_pixels
    }

def check_image(path):
    print(f"\n==========================================")
    print(f"Inspecting PNG: {path}")
    print(f"==========================================")
    w, h, rgba = decode_png_pixels(path)
    res = calculate_shannon_entropy(rgba)
    print(f"Dimensions: {w}x{h} ({w*h} pixels)")
    print(f"Unique RGBA Colors: {res['unique_colors']}")
    print(f"Pixel Shannon Entropy H: {res['entropy']:.4f} bits/pixel")
    print(f"Non-Zero Pixels: {res['non_zero_pixels']}/{res['total_pixels']} ({res['non_zero_ratio']*100:.2f}%)")
    
    # Analyze image boundaries
    # Top 10%
    top_pixels = rgba[: int(w * h * 0.10) * 4]
    top_active = sum(1 for i in range(0, len(top_pixels), 4) if any(top_pixels[i+c] != 0 for c in range(4)))
    # Bottom 10%
    bot_pixels = rgba[int(w * h * 0.90) * 4 :]
    bot_active = sum(1 for i in range(0, len(bot_pixels), 4) if any(bot_pixels[i+c] != 0 for c in range(4)))
    
    # Left edge (first 5 pixels of each row)
    left_active = sum(1 for y in range(h) if any(any(rgba[(y*w + x)*4 + c] != 0 for c in range(4)) for x in range(min(5, w))))
    # Right edge (last 5 pixels of each row)
    right_active = sum(1 for y in range(h) if any(any(rgba[(y*w + (w-1-x))*4 + c] != 0 for c in range(4)) for x in range(min(5, w))))
    
    print(f"Top 10% active pixels: {top_active}/{len(top_pixels)//4} ({top_active/(len(top_pixels)//4)*100:.2f}%)")
    print(f"Bottom 10% active pixels: {bot_active}/{len(bot_pixels)//4} ({bot_active/(len(bot_pixels)//4)*100:.2f}%)")
    print(f"Left edge active rows: {left_active}/{h} ({left_active/h*100:.2f}%)")
    print(f"Right edge active rows: {right_active}/{h} ({right_active/h*100:.2f}%)")
    
    assert res['entropy'] >= 1.0, f"Entropy {res['entropy']} < 1.0"
    assert res['non_zero_pixels'] > 0, "Image is blank"
    assert bot_active > 0, "Bottom 10% is completely empty"
    print(f"✔ [PASS] {path} is valid and meets all empirical entropy/boundary criteria.")

def main():
    check_image('screenshot.png')
    check_image('dist/screenshot.png')
    print("\n⚡ ALL PNG DECODE AND ENTROPY CHECKS PASSED EMPIRICALLY!")

if __name__ == '__main__':
    main()
