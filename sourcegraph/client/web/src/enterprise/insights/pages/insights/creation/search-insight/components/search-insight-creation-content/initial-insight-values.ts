import { createDefaultEditSeries } from '../../../../../../components'
import { CreateInsightFormFields } from '../../types'

export const INITIAL_INSIGHT_VALUES: CreateInsightFormFields = {
    // If user opens the creation form to create insight
    // we want to show the series form as soon as possible
    // and do not force the user to click the 'add another series' button
    series: [createDefaultEditSeries({ edit: true })],
    step: 'months',
    stepValue: '2',
    title: '',
    repositories: '',
    allRepos: false,
    dashboardReferenceCount: 0,
}
