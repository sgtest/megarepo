package graphqlbackend

import "github.com/sourcegraph/sourcegraph/internal/markdown"

type Markdown string

func (m Markdown) Text() string {
	return string(m)
}

func (m Markdown) HTML() string {
	return markdown.Render(string(m))
}
