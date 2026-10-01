/** Logo with a tagline underneath; right-aligned on desktop to sit beside a column. */
export function Brand({ tagline }: { tagline: string }) {
  return (
    <div className="flex flex-col items-center gap-3 text-center md:items-end md:text-right">
      <h1 className="font-logo text-4xl font-bold tracking-wider uppercase md:text-5xl">
        stowarzysz
      </h1>
      <p className="text-base">{tagline}</p>
    </div>
  )
}
