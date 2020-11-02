import React, { Fragment, useMemo } from 'react'
import { parseSearchQuery } from '../../../shared/src/search/parser/parser'

// A read-only syntax highlighted search query
export const SyntaxHighlightedSearchQuery: React.FunctionComponent<{ query: string }> = ({ query }) => {
    const tokens = useMemo(() => {
        const parsedQuery = parseSearchQuery(query)
        return parsedQuery.type === 'success'
            ? parsedQuery.token.members.map(token => {
                  if (token.type === 'filter') {
                      return (
                          <Fragment key={token.range.start}>
                              <span className="search-keyword">
                                  {query.slice(token.filterType.range.start, token.filterType.range.end)}:
                              </span>
                              {token.filterValue ? (
                                  <>{query.slice(token.filterValue.range.start, token.filterValue.range.end)}</>
                              ) : null}
                          </Fragment>
                      )
                  }
                  if (token.type === 'operator') {
                      return (
                          <span className="search-operator" key={token.range.start}>
                              {query.slice(token.range.start, token.range.end)}
                          </span>
                      )
                  }
                  return <Fragment key={token.range.start}>{query.slice(token.range.start, token.range.end)}</Fragment>
              })
            : [<Fragment key="0">{query}</Fragment>]
    }, [query])

    return <span className="text-monospace search-query-link">{tokens}</span>
}
