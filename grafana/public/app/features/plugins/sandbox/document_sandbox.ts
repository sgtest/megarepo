import { ProxyTarget } from '@locker/near-membrane-shared';

import { forbiddenElements } from './constants';

// IMPORTANT: NEVER export this symbol from a public (e.g `@grafana/*`) package
const SANDBOX_LIVE_VALUE = Symbol.for('@@SANDBOX_LIVE_VALUE');

export function getSafeSandboxDomElement(element: Element, pluginId: string): Element {
  const nodeName = Reflect.get(element, 'nodeName');

  // the condition redundancy is intentional
  if (nodeName === 'body' || element === document.body) {
    return document.body;
  }

  // allow access to the head
  // the condition redundancy is intentional
  if (nodeName === 'head' || element === document.head) {
    return element;
  }

  // allow access to the HTML element
  if (element === document.documentElement) {
    return element;
  }

  if (forbiddenElements.includes(nodeName)) {
    throw new Error('<' + nodeName + '> is not allowed in sandboxed plugins');
  }

  // allow elements inside the sandbox or the sandbox body
  if (isDomElementInsideSandbox(element, pluginId)) {
    return element;
  }

  if (element.parentNode === document.body || element.closest('#reactRoot') === null) {
    return element;
  }

  // any other element gets a mock
  const mockElement = document.createElement(nodeName);
  mockElement.dataset.grafanaPluginSandboxElement = 'true';
  return mockElement;
}

export function isDomElement(obj: unknown): obj is Element {
  if (typeof obj === 'object' && obj instanceof Element) {
    try {
      return obj.nodeName !== undefined;
    } catch (e) {
      return false;
    }
  }
  return false;
}

/**
 * Mark an element style attribute as a live target inside the sandbox
 * A "live target" is an object which attributes can be observed
 * and modified directly inside the sandbox
 *
 * This is necessary for plugins working with style attributes to work in Chrome
 */
export function markDomElementStyleAsALiveTarget(el: Element) {
  if (
    // only HTMLElement's (extends Element) have a style attribute
    el instanceof HTMLElement &&
    // do not define it twice
    !Object.hasOwn(el.style, SANDBOX_LIVE_VALUE)
  ) {
    Reflect.defineProperty(el.style, SANDBOX_LIVE_VALUE, {});
  }
}

export function isLiveTarget(el: ProxyTarget) {
  return Object.hasOwn(el, SANDBOX_LIVE_VALUE);
}

/*
 * An element is considered to be inside the sandbox if:
 * - is not part of the document (detached)
 * - is inside a div[data-plugin-sandbox]
 *
 */
export function isDomElementInsideSandbox(el: Element, pluginId: string): boolean {
  return !document.contains(el) || el.closest(`[data-plugin-sandbox=${pluginId}]`) !== null;
}

let sandboxBody: HTMLDivElement;

export function getSandboxMockBody(): Element {
  if (!sandboxBody) {
    sandboxBody = document.createElement('div');
    sandboxBody.setAttribute('id', 'grafana-plugin-sandbox-body');

    // the following dataset redundancy is intentional
    sandboxBody.setAttribute('data-plugin-sandbox', 'true');
    sandboxBody.dataset.pluginSandbox = 'sandboxed-plugin';

    sandboxBody.style.width = '100%';
    sandboxBody.style.height = '0%';
    sandboxBody.style.overflow = 'hidden';
    sandboxBody.style.top = '0';
    sandboxBody.style.left = '0';
    document.body.appendChild(sandboxBody);
  }
  return sandboxBody;
}
