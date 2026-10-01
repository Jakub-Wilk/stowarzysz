/** Mirrors the server's limit (accounts/avatars.py); the server stays the authority. */
export const MAX_IMAGE_BYTES = 5 * 1024 * 1024

/** A picture chosen while creating a user; uploaded once the account exists. */
export interface PendingPicture {
  file: File
  /** data: URL for the preview. */
  preview: string
}

export function validateImage(file: File): string | null {
  if (!file.type.startsWith('image/')) return 'Choose an image file.'
  if (file.size > MAX_IMAGE_BYTES) {
    return `Image is too large (max ${MAX_IMAGE_BYTES / (1024 * 1024)} MB).`
  }
  return null
}

export function readPreview(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(reader.error)
    reader.readAsDataURL(file)
  })
}
