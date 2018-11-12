export function injectSourcegraphApp(marker: HTMLElement): void {
    if (document.getElementById(marker.id)) {
        return
    }

    // Generate and insert DOM element, in case this code executes first.
    document.body.appendChild(marker)

    window.addEventListener('load', () => {
        dispatchSourcegraphEvents()
    })

    if (document.readyState === 'complete' || document.readyState === 'interactive') {
        dispatchSourcegraphEvents()
    }
}

function dispatchSourcegraphEvents(): void {
    // Send custom webapp <-> extension registration event in case webapp listener is attached first.
    document.dispatchEvent(new CustomEvent<{}>('sourcegraph:browser-extension-registration'))
}
