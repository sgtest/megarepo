import { TextDocumentDecoration } from '@sourcegraph/extension-api-types'
import { Observable } from 'rxjs'
import { map, switchMap } from 'rxjs/operators'
import {
    DecorationAttachmentRenderOptions,
    ThemableDecorationAttachmentStyle,
    ThemableDecorationStyle,
    BadgeAttachmentRenderOptions,
    ThemableBadgeAttachmentStyle,
} from 'sourcegraph'
import { combineLatestOrDefault } from '../../../util/rxjs/combineLatestOrDefault'
import { TextDocumentIdentifier } from '../types/textDocument'
import { FeatureProviderRegistry } from './registry'
import { flattenAndCompact } from './util'

export type ProvideTextDocumentDecorationSignature = (
    textDocument: TextDocumentIdentifier
) => Observable<TextDocumentDecoration[] | null>

/** Provides text document decorations from all extensions. */
export class TextDocumentDecorationProviderRegistry extends FeatureProviderRegistry<
    undefined,
    ProvideTextDocumentDecorationSignature
> {
    public getDecorations(parameters: TextDocumentIdentifier): Observable<TextDocumentDecoration[] | null> {
        return getDecorations(this.providers, parameters)
    }
}

/**
 * Returns an observable that emits all decorations whenever any of the last-emitted set of providers emits
 * decorations.
 *
 * Most callers should use TextDocumentDecorationProviderRegistry, which uses the registered decoration providers.
 */
export function getDecorations(
    providers: Observable<ProvideTextDocumentDecorationSignature[]>,
    parameters: TextDocumentIdentifier
): Observable<TextDocumentDecoration[] | null> {
    return providers
        .pipe(switchMap(providers => combineLatestOrDefault(providers.map(provider => provider(parameters)))))
        .pipe(map(flattenAndCompact))
}

/**
 * Resolves the actual styles to use for the attachment based on the current theme.
 */
export function decorationStyleForTheme(
    attachment: TextDocumentDecoration,
    isLightTheme: boolean
): ThemableDecorationStyle {
    const overrides = isLightTheme ? attachment.light : attachment.dark
    // Discard non-ThemableDecorationStyle properties so they aren't included in result.
    const { range, isWholeLine, after, light, dark, ...base } = attachment
    return { ...base, ...overrides }
}

/**
 * Resolves the actual styles to use for the attachment based on the current theme.
 */
export function decorationAttachmentStyleForTheme(
    attachment: DecorationAttachmentRenderOptions,
    isLightTheme: boolean
): ThemableDecorationAttachmentStyle {
    const overrides = isLightTheme ? attachment.light : attachment.dark
    // Discard non-ThemableDecorationAttachmentStyle properties so they aren't included in result.
    const { contentText, hoverMessage, linkURL, light, dark, ...base } = attachment
    return { ...base, ...overrides }
}

/**
 * Resolves the actual styles to use for the badge attachment based on the current theme.
 */
export function badgeAttachmentStyleForTheme(
    attachment: BadgeAttachmentRenderOptions,
    isLightTheme: boolean
): ThemableBadgeAttachmentStyle {
    const overrides = isLightTheme ? attachment.light : attachment.dark
    // Discard non-ThemableDecorationAttachmentStyle properties so they aren't included in result.
    const { hoverMessage, linkURL, light, dark, ...base } = attachment
    return { ...base, ...overrides }
}

export type DecorationMapByLine = ReadonlyMap<number, TextDocumentDecoration[]>

/**
 * @returns Map from 1-based line number to non-empty array of TextDocumentDecoration for that line
 *
 * @todo this does not handle decorations that span multiple lines
 */
export const groupDecorationsByLine = (decorations: TextDocumentDecoration[] | null): DecorationMapByLine => {
    const grouped = new Map<number, TextDocumentDecoration[]>()
    for (const decoration of decorations || []) {
        const lineNumber = decoration.range.start.line + 1
        const decorationsForLine = grouped.get(lineNumber)
        if (!decorationsForLine) {
            grouped.set(lineNumber, [decoration])
        } else {
            decorationsForLine.push(decoration)
        }
    }
    return grouped
}
