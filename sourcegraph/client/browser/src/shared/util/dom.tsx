/**
 * Inserts an element after the reference node.
 * @param el The element to be rendered.
 * @param referenceNode The node to render the element after.
 */
export function insertAfter(el: HTMLElement, referenceNode: Node): void {
    if (referenceNode.parentNode) {
        referenceNode.parentNode.insertBefore(el, referenceNode.nextSibling)
    }
}

export function isMouseEventWithModifierKey(e: MouseEvent): boolean {
    return e.altKey || e.shiftKey || e.ctrlKey || e.metaKey || e.which === 2
}

/**
 * commitIDFromPermalink finds the permalink element on the page and extracts
 * the 40 character commit ID from it. This will throw if the link doesn't exist
 * or doesn't match the provided regex.
 */
export function commitIDFromPermalink({ selector, hrefRegex }: { selector: string; hrefRegex: RegExp }): string {
    const permalinkElement = document.querySelector<HTMLAnchorElement>(selector)
    if (!permalinkElement) {
        throw new Error(
            `Unable to determine the commit ID (40 character hash) you're on because the permalink shortcut element (query selector ${selector}) which contains the commit ID does not exist in the DOM.`
        )
    }
    const href = permalinkElement.getAttribute('href')
    if (!href) {
        throw new Error(
            `Unable to determine the commit ID (40 character hash) you're on because the permalink shortcut element (query selector ${selector}) which contains the commit ID does not have an href attribute.`
        )
    }
    const commitIDMatch = hrefRegex.exec(href)
    if (!commitIDMatch || !commitIDMatch[1]) {
        throw new Error(
            `Unable to determine the commit ID (40 character hash) you're on because the permalink shortcut element's (query selector ${selector}) href is ${href}, which doesn't match the regex /${hrefRegex}/.`
        )
    }
    return commitIDMatch[1]
}
