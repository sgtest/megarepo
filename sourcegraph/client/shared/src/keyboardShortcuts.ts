import { Key, ModifierKey } from '@sourcegraph/shared/src/react-shortcuts'

/**
 * An action and its associated keybindings.
 */
export interface KeyboardShortcut {
    /** A descriptive title. */
    title: string

    /** The keybindings that trigger this shortcut. */
    keybindings: Keybinding[]

    /** If set, do not show this in the KeyboardShortcutsHelp modal. */
    hideInHelp?: boolean
}

/** A key sequence (that triggers a keyboard shortcut). */
export interface Keybinding {
    /** Keys that must be held down. */
    held?: (ModifierKey | 'Mod')[]

    /** Keys that must be pressed in order (when holding the `held` keys). */
    ordered: Key[]
}
