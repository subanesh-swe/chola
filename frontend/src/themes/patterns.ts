/**
 * Background patterns — repeating tiles that paint behind the app background.
 *
 * Each entry produces a CSS `background-image` value (either a gradient or a
 * data: URL) that gets layered above the solid app color and below content.
 *
 * Patterns adapt to the current theme automatically: they use
 * `currentColor`-style refs to text tokens so they tint with the palette.
 */

export interface PatternDef {
  id: string;
  name: string;
  /** CSS `background-image` value. May reference CSS vars (e.g. var(--color-text-muted)). */
  image: string;
  /** CSS `background-size` value (e.g. "20px 20px"). */
  size: string;
  /** 0…1 — opacity multiplier baked into the image OR overridden in CSS. */
  opacity: number;
}

// Helper to encode an inline SVG as a data: URL safe for CSS use.
// SVG uses currentColor; we set color via wrapper. But since background-image
// can't inherit color from its host, we hard-code the color in the SVG.
// Instead we use color-mix-friendly hex with low alpha so it works on
// both light and dark themes — caller picks one pattern, picks one color.
function svg(content: string): string {
  return `url("data:image/svg+xml;utf8,${encodeURIComponent(content)}")`;
}

export const PATTERNS: PatternDef[] = [
  {
    id: 'none',
    name: 'None',
    image: 'none',
    size: 'auto',
    opacity: 0,
  },
  {
    id: 'dots',
    name: 'Dots',
    // Tiny dots, low-opacity, derived from the muted text color
    image: 'radial-gradient(circle, color-mix(in srgb, currentColor 20%, transparent) 1px, transparent 1px)',
    size: '18px 18px',
    opacity: 0.5,
  },
  {
    id: 'grid',
    name: 'Grid',
    image:
      'linear-gradient(to right, color-mix(in srgb, currentColor 8%, transparent) 1px, transparent 1px),' +
      ' linear-gradient(to bottom, color-mix(in srgb, currentColor 8%, transparent) 1px, transparent 1px)',
    size: '32px 32px',
    opacity: 0.7,
  },
  {
    id: 'diagonal',
    name: 'Diagonal lines',
    image:
      'repeating-linear-gradient(45deg, color-mix(in srgb, currentColor 6%, transparent) 0px, color-mix(in srgb, currentColor 6%, transparent) 1px, transparent 1px, transparent 14px)',
    size: 'auto',
    opacity: 1,
  },
  {
    id: 'crosshatch',
    name: 'Cross-hatch',
    image:
      'repeating-linear-gradient(45deg,  color-mix(in srgb, currentColor 5%, transparent) 0 1px, transparent 1px 10px),' +
      ' repeating-linear-gradient(-45deg, color-mix(in srgb, currentColor 5%, transparent) 0 1px, transparent 1px 10px)',
    size: 'auto',
    opacity: 1,
  },
  {
    id: 'doodle',
    name: 'Doodle',
    // Small geometric shapes — circles and triangles at low opacity, tiled at 80px.
    image: svg(
      `<svg xmlns='http://www.w3.org/2000/svg' width='80' height='80' viewBox='0 0 80 80'>
        <g fill='none' stroke='%23ffffff' stroke-opacity='0.06' stroke-width='1.2'>
          <circle cx='12' cy='12' r='6'/>
          <circle cx='52' cy='28' r='9'/>
          <circle cx='20' cy='56' r='5'/>
          <circle cx='66' cy='62' r='7'/>
          <path d='M40 0 l8 14 l-16 0 z'/>
          <path d='M72 38 l5 9 l-10 0 z'/>
          <path d='M4 40 l4 7 l-8 0 z'/>
        </g>
      </svg>`,
    ),
    size: '80px 80px',
    opacity: 1,
  },
  {
    id: 'stripes',
    name: 'Stripes',
    // Subtle horizontal stripes.
    image:
      'repeating-linear-gradient(0deg, color-mix(in srgb, currentColor 4%, transparent) 0 1px, transparent 1px 28px)',
    size: 'auto',
    opacity: 1,
  },
  {
    id: 'topo',
    name: 'Topo (contour)',
    // SVG topographic curves
    image: svg(
      `<svg xmlns='http://www.w3.org/2000/svg' width='120' height='120' viewBox='0 0 120 120'>
        <g fill='none' stroke='%23ffffff' stroke-opacity='0.05' stroke-width='1'>
          <path d='M -10 50 Q 30 20 60 50 T 130 50'/>
          <path d='M -10 70 Q 30 40 60 70 T 130 70'/>
          <path d='M -10 90 Q 30 60 60 90 T 130 90'/>
          <path d='M -10 30 Q 30  0 60 30 T 130 30'/>
        </g>
      </svg>`,
    ),
    size: '120px 120px',
    opacity: 1,
  },
];

export const DEFAULT_PATTERN_ID = 'none';

export function getPattern(id: string): PatternDef {
  return PATTERNS.find((p) => p.id === id) ?? PATTERNS[0];
}
