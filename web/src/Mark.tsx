/**
 * The product's mark: a hex tile with the line's monogram in it.
 *
 * Inline rather than an `<img>` pointing at the favicon: the header is the
 * first thing painted, and a mark that arrives on its own request is a mark
 * that flickers in after the page. The shape is the same one in
 * `assets/logo-s.svg` and in the line's brand registry.
 *
 * The colour is not written here. `text-accent` is hilvan's product colour in
 * the line's vocabulary, and `currentColor` follows it - so the tile stays
 * right if the accent ever moves, and the theme keeps its say. A hex value in
 * this file would be the one place that did not.
 */

export function Mark({ size = 22 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 100 100"
      aria-hidden="true"
      focusable="false"
      className="shrink-0 text-accent"
    >
      <polygon
        points="50,5 89,27.5 89,72.5 50,95 11,72.5 11,27.5"
        fill="currentColor"
        stroke="currentColor"
        strokeWidth="9"
        strokeLinejoin="round"
      />
      {/* The monogram is cut out of the tile rather than drawn over it, so it
          reads on whatever the accent turns out to be, light theme or dark. */}
      <text
        x="50"
        y="66"
        textAnchor="middle"
        fontFamily='"Cascadia Code","JetBrains Mono",Consolas,ui-monospace,monospace'
        fontWeight="800"
        fontSize="46"
        className="fill-bg"
      >
        hv
      </text>
    </svg>
  )
}
