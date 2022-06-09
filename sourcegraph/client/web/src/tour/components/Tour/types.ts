/**
 * Tour supported languages
 */
export enum TourLanguage {
    C = 'C',
    Go = 'Go',
    Java = 'Java',
    Javascript = 'JavaScript',
    Php = 'PHP',
    Python = 'Python',
    Typescript = 'TypeScript',
}

/**
 * Tour task
 */
export interface TourTaskType {
    title?: string
    dataAttributes?: {}
    icon?: React.ReactNode
    steps: TourTaskStepType[]
    /**
     * Completion percentage, 0-100. Dynamically calculated field
     */
    completed?: number
}

export interface TourTaskStepType {
    id: string
    label: string
    tooltip?: string
    action:
        | {
              type: 'video'
              value: string | Record<TourLanguage, string>
          }
        | {
              type: 'link' | 'new-tab-link'
              variant?: 'button-primary'
              value: string | Record<TourLanguage, string>
          }
        | {
              type: 'restart'
              value: string
          }
    /**
     * HTML string, which will be displayed in info box when navigating to a step link.
     */
    info?: JSX.Element
    /**
     * The step will be marked as completed only if one of the "completeAfterEvents" will be triggered
     */
    completeAfterEvents?: string[]
    /**
     * Dynamically calculated field
     */
    isCompleted?: boolean
}
