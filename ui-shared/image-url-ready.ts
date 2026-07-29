export function imageUrlReady(url: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.decoding = "async";
    image.addEventListener("load", () => resolve(url), { once: true });
    image.addEventListener("error", () => reject(new Error("Image preview unavailable")), {
      once: true,
    });
    image.src = url;
  });
}
