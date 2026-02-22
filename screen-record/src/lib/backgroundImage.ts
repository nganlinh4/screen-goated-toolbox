export async function normalizeBackgroundImageToDataUrl(
  source: File | string
): Promise<string> {
  const objectUrl = source instanceof File ? URL.createObjectURL(source) : null;
  const src: string = objectUrl ?? (typeof source === 'string' ? source : '');

  try {
    const img = await loadImage(src);
    const canvas = document.createElement('canvas');
    canvas.width = img.naturalWidth || img.width;
    canvas.height = img.naturalHeight || img.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) throw new Error('Failed to get canvas context');
    ctx.drawImage(img, 0, 0);
    // Lossless normalization for consistent decode/render path without quality reduction.
    return canvas.toDataURL('image/png');
  } finally {
    if (objectUrl) URL.revokeObjectURL(objectUrl);
  }
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error(`Failed to load image: ${src}`));
    img.src = src;
  });
}
