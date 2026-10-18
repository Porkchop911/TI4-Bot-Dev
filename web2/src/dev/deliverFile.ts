/**
 * Gives a file to the player: on a phone the share sheet of the system (an installed app has no
 * downloads bar), else a download.
 */
export async function deliverFile(filename: string, blob: Blob) {
  const file = new File([blob], filename, { type: blob.type });
  if (matchMedia("(pointer: coarse)").matches && navigator.canShare?.({ files: [file] })) {
    try {
      await navigator.share({ files: [file], title: filename });
      return;
    } catch (error) {
      // The player closed the sheet: nothing to do. Another failure falls back to a download.
      if (error instanceof DOMException && error.name === "AbortError") {
        return;
      }
    }
  }
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = filename;
  link.click();
  // A link that is revoked at once may not download yet.
  setTimeout(() => URL.revokeObjectURL(link.href), 10_000);
}
