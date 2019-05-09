enum AppEnv {
    Extension,
    Page,
}

enum ScriptEnv {
    Content,
    Background,
    Options,
}

interface AppContext {
    appEnv: AppEnv
    scriptEnv: ScriptEnv
}

const options = /options\.html/

function getContext(): AppContext {
    const appEnv = window.SG_ENV === 'EXTENSION' ? AppEnv.Extension : AppEnv.Page

    let scriptEnv: ScriptEnv = ScriptEnv.Content
    if (appEnv === AppEnv.Extension) {
        if (options.test(window.location.pathname)) {
            scriptEnv = ScriptEnv.Options
        } else if (globalThis.browser && browser.runtime.getBackgroundPage) {
            scriptEnv = ScriptEnv.Background
        }
    }

    return {
        appEnv,
        scriptEnv,
    }
}

const ctx = getContext()

export const isBackground = ctx.scriptEnv === ScriptEnv.Background
export const isOptions = ctx.scriptEnv === ScriptEnv.Options

export const isExtension = ctx.appEnv === AppEnv.Extension
export const isInPage = !isExtension

export const isPhabricator = Boolean(document.querySelector('.phabricator-wordmark'))
