import { Remote } from 'comlink'
import { Observable } from 'rxjs'

import { FlatExtensionHostAPI } from '@sourcegraph/shared/src/api/contract'
import { ViewProviderResult } from '@sourcegraph/shared/src/api/extension/extensionHostApi'
import { PlatformContext } from '@sourcegraph/shared/src/platform/context'

export enum ViewInsightProviderSourceType {
    Backend = 'Backend',
    Extension = 'Extension',
}

export interface ViewInsightProviderResult extends ViewProviderResult {
    /** The source of view provider to distinguish between data from extension and data from backend */
    source: ViewInsightProviderSourceType
}

export interface SubjectSettingsResult {
    id: number | null
    contents: string
}

export interface ApiService {
    getCombinedViews: (
        getExtensionsInsights: () => Observable<ViewProviderResult[]>
    ) => Observable<ViewInsightProviderResult[]>
    getInsightCombinedViews: (
        extensionApi: Promise<Remote<FlatExtensionHostAPI>>
    ) => Observable<ViewInsightProviderResult[]>
    getSubjectSettings: (id: string) => Observable<SubjectSettingsResult>
    updateSubjectSettings: (
        context: Pick<PlatformContext, 'updateSettings'>,
        subjectId: string,
        content: string
    ) => Observable<void>
}
