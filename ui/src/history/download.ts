// Saving a file the server sent, through the browser's download.

/** Saves `blob` as `name`, as a download. */
export function download(name: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  document.body.append(link);
  link.click();
  link.remove();
  // Some browsers start the download after click returns; let it.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
