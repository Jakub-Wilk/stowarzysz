import { Children, type CSSProperties, type ReactNode } from 'react'

import { cn } from '@/lib/utils'

/**
 * Fades each child up in sequence. Wrap list items; the delay is capped by the `stagger`
 * utility (index.css), so long lists don't lag. Children are rendered inside a fragment-like
 * wrapper per item, so pass keyed elements.
 */
export function Stagger({
  children,
  as: Tag = 'div',
  className,
  itemClassName,
}: {
  children: ReactNode
  as?: 'div' | 'ul' | 'ol'
  className?: string
  itemClassName?: string
}) {
  const Item = Tag === 'div' ? 'div' : 'li'
  return (
    <Tag className={className}>
      {Children.toArray(children).map((child, i) => (
        <Item
          key={typeof child === 'object' && 'key' in child ? child.key : i}
          className={cn('stagger animate-fade-up', itemClassName)}
          style={{ '--i': i } as CSSProperties}
        >
          {child}
        </Item>
      ))}
    </Tag>
  )
}
