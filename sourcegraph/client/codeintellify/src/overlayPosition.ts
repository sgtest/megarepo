// eslint-disable-next-line unicorn/prevent-abbreviations
export interface HasGetBoundingClientRect {
    getBoundingClientRect: () => DOMRect
}

export interface CalculateOverlayPositionOptions {
    /** The closest parent element that is `position: relative` */
    relativeElement: HasGetBoundingClientRect & { scrollLeft: number; scrollTop: number }
    /** The DOM Node that was hovered */
    target: HasGetBoundingClientRect
    /** The DOM Node of the tooltip */
    hoverOverlayElement: HasGetBoundingClientRect
}

export type CSSOffsets = { left: number } & ({ top: number } | { bottom: number })

/**
 * Calculates the desired position of the hover overlay depending on the container,
 * the hover target and the size of the hover overlay
 */
export const calculateOverlayPosition = ({
    relativeElement,
    target,
    hoverOverlayElement,
}: CalculateOverlayPositionOptions): CSSOffsets => {
    const relativeElementBounds = relativeElement.getBoundingClientRect()
    const targetBounds = target.getBoundingClientRect()
    const hoverOverlayBounds = hoverOverlayElement.getBoundingClientRect()

    // If the relativeElement is scrolled horizontally, we need to account for the offset (if not scrollLeft will be 0)
    const relativeHoverOverlayLeft = targetBounds.left + relativeElement.scrollLeft - relativeElementBounds.left

    // Check if the top of the hover overlay would be outside of the relative element or the viewport
    if (targetBounds.top - hoverOverlayBounds.height < Math.max(relativeElementBounds.top, 0)) {
        // Position it below the target
        // If the relativeElement is scrolled, we need to account for the offset (if not scrollTop will be 0)
        return {
            left: relativeHoverOverlayLeft,
            top: targetBounds.bottom - relativeElementBounds.top + relativeElement.scrollTop,
        }
    }

    // Else position it above the target
    // If the relativeElement is scrolled, we need to account for the offset (if not scrollTop will be 0)
    return {
        left: relativeHoverOverlayLeft,
        bottom:
            relativeElementBounds.height - (targetBounds.top - relativeElementBounds.top + relativeElement.scrollTop),
    }
}
