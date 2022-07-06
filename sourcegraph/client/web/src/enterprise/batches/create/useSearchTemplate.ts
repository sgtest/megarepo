import { useLocation } from 'react-router'

import helloWorldSample from '../batch-spec/edit/library/hello-world.batch.yaml'
import { insertQueryIntoLibraryItem, insertNameIntoLibraryItem } from '../batch-spec/yaml-util'

interface UseSearchTemplateResult {
    searchQuery?: string
    renderTemplate?: (title: string) => string
}

const createRenderTemplate = (query: string): ((title: string) => string) => {
    let template: string
    template = insertQueryIntoLibraryItem(helloWorldSample, query)
    template = template.replace(
        '# Find all repositories that contain a README.md file.',
        '# This is your query from search'
    )

    return title => insertNameIntoLibraryItem(template, title)
}

/**
 * Custom hook for create page which creates a batch spec from a search query
 */
export const useSearchTemplate = (): UseSearchTemplateResult => {
    const location = useLocation()
    const parameters = new URLSearchParams(location.search)

    const query = parameters.get('q')
    const patternType = parameters.get('patternType')

    if (query) {
        const searchQuery = `${query} ${patternType ? `patternType:${patternType}` : ''}`
        const renderTemplate = createRenderTemplate(searchQuery)
        return { renderTemplate, searchQuery }
    }

    return { renderTemplate: undefined, searchQuery: undefined }
}
