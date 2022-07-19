package check

import (
	"context"
	"fmt"
	"io"
	"strings"
	"sync"
	"time"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/analytics"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/group"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

type SuggestFunc[Args any] func(category string, c *Check[Args], err error) string

type Runner[Args any] struct {
	Input      io.Reader
	Output     *std.Output
	Categories []Category[Args]

	// RenderDescription sets a description to render before core check loops, such as a
	// massive ASCII art thing.
	RenderDescription func(*std.Output)
	// GenerateAnnotations toggles whether check execution should render annotations to
	// the './annotations' directory.
	GenerateAnnotations bool
	// RunPostFixChecks toggles whether to run checks again after a fix is applied.
	RunPostFixChecks bool
	// AnalyticsCategory is the category to track analytics with.
	AnalyticsCategory string
	// Concurrency controls the maximum number of checks across categories to evaluate at
	// the same time - defaults to 10.
	Concurrency int
	// SuggestOnCheckFailure can be implemented to prompt the user to try certain things
	// if a check fails. The suggestion string can be in Markdown.
	SuggestOnCheckFailure SuggestFunc[Args]
}

// NewRunner creates a Runner for executing checks and applying fixes in a variety of ways.
// It is a convenience function that indicates the required fields that must be provided
// to a Runner - fields can also be set directly on the struct.
func NewRunner[Args any](in io.Reader, out *std.Output, categories []Category[Args]) *Runner[Args] {
	return &Runner[Args]{
		Input:       in,
		Output:      out,
		Categories:  categories,
		Concurrency: 10,
	}
}

// Check executes all checks exactly once and exits.
func (r *Runner[Args]) Check(
	ctx context.Context,
	args Args,
) error {
	results := r.runAllCategoryChecks(ctx, args)
	if len(results.failed) > 0 {
		if len(results.skipped) > 0 {
			return errors.Newf("%d checks failed (%d skipped)", len(results.failed), len(results.skipped))
		}
		return errors.Newf("%d checks failed", len(results.failed))
	}

	return nil
}

// Fix attempts to applies available fixes on checks that are not satisfied.
func (r *Runner[Args]) Fix(
	ctx context.Context,
	args Args,
) error {
	// Get state
	results := r.runAllCategoryChecks(ctx, args)
	if len(results.failed) == 0 {
		// Nothing failed, we're good to go!
		return nil
	}

	r.Output.WriteNoticef("Attempting to fix %d failed categories", len(results.failed))
	for _, i := range results.failed {
		category := r.Categories[i]

		ok := r.fixCategoryAutomatically(ctx, i+1, &category, args, results)
		results.categories[category.Name] = ok
	}

	// Report what is still bust
	failedCategories := []string{}
	for c, ok := range results.categories {
		if ok {
			continue
		}
		failedCategories = append(failedCategories, fmt.Sprintf("%q", c))
	}
	if len(failedCategories) > 0 {
		return errors.Newf("Some categories are still unsatisfied: %s", strings.Join(failedCategories, ", "))
	}

	return nil
}

// Interactive runs both checks and fixes in an interactive manner, prompting the user for
// decisions about which fixes to apply.
func (r *Runner[Args]) Interactive(
	ctx context.Context,
	args Args,
) error {
	// Keep interactive runner up until all issues are fixed or the user exits
	results := &runAllCategoryChecksResult{
		failed: []int{1}, // initialize, this gets reset immediately
	}
	for len(results.failed) != 0 {
		// Update results
		results = r.runAllCategoryChecks(ctx, args)
		if len(results.failed) == 0 {
			break
		}

		r.Output.WriteWarningf("Some checks failed. Which one do you want to fix?")

		idx, err := getNumberOutOf(r.Input, r.Output, results.failed)
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
		selectedCategory := r.Categories[idx]

		r.Output.ClearScreen()

		err = r.presentFailedCategoryWithOptions(ctx, idx, &selectedCategory, args, results)
		if err != nil {
			if err == io.EOF {
				return nil // we are done
			}

			r.Output.WriteWarningf("Encountered error while fixing: %s", err.Error())
			// continue, do not exit
		}
	}

	return nil
}

// runAllCategoryChecksResult provides a summary of categories checks results.
type runAllCategoryChecksResult struct {
	all     []int
	failed  []int
	skipped []int

	// Indicates whether each category succeeded or not
	categories map[string]bool
}

var errSkipped = errors.New("skipped")

// runAllCategoryChecks is the main entrypoint for running the checks in this runner.
func (r *Runner[Args]) runAllCategoryChecks(ctx context.Context, args Args) *runAllCategoryChecksResult {
	if r.RenderDescription != nil {
		r.RenderDescription(r.Output)
	}

	statuses := []*output.StatusBar{}
	var checks int
	for i, category := range r.Categories {
		statuses = append(statuses, output.NewStatusBarWithLabel(fmt.Sprintf("%d. %s", i+1, category.Name)))
		checks += len(category.Checks)
	}
	progress := r.Output.ProgressWithStatusBars([]output.ProgressBar{{
		Label: "Running checks",
		Max:   float64(checks),
	}}, statuses, nil)

	var (
		start           = time.Now()
		categoriesGroup = group.NewWithStreaming[error]()

		// checksLimiter is shared to limit all concurrent checks across categories.
		checksLimiter = group.NewBasicLimiter(r.Concurrency)

		// aggregated results
		categoriesSkipped   = map[int]bool{}
		categoriesDurations = map[int]time.Duration{}

		// used for progress bar - needs to be thread-safe since it can be updated from
		// multiple categories at once.
		progressMu           sync.Mutex
		checksDone           float64
		updateChecksProgress = func() {
			progressMu.Lock()
			defer progressMu.Unlock()

			checksDone += 1
			progress.SetValue(0, checksDone)
		}
		updateCheckSkipped = func(i int, checkName string, err error) {
			progressMu.Lock()
			defer progressMu.Unlock()

			progress.StatusBarUpdatef(i, "Check %s skipped: %s", checkName, err.Error())
		}
		updateCheckFailed = func(i int, checkName string, err error) {
			progressMu.Lock()
			defer progressMu.Unlock()

			errParts := strings.SplitN(err.Error(), "\n", 2)
			if len(errParts) > 2 {
				// truncate to one line - writing multple lines causes some jank
				errParts[0] += " ..."
			}
			progress.StatusBarFailf(i, "Check %s failed: %s", checkName, errParts[0])
		}
		updateCategoryStarted = func(i int) {
			progressMu.Lock()
			defer progressMu.Unlock()
			progress.StatusBarUpdatef(i, "Running checks...")
		}
		updateCategorySkipped = func(i int, err error) {
			progressMu.Lock()
			defer progressMu.Unlock()

			progress.StatusBarCompletef(i, "Category skipped: %s", err.Error())
		}
		updateCategoryCompleted = func(i int) {
			progressMu.Lock()
			defer progressMu.Unlock()
			progress.StatusBarCompletef(i, "Done!")
		}
	)

	for i, category := range r.Categories {
		updateCategoryStarted(i)

		// Copy
		i, category := i, category

		// Run categories concurrently
		categoriesGroup.Go(func() error {
			if err := category.CheckEnabled(ctx, args); err != nil {
				// Mark as done
				updateCategorySkipped(i, err)
				return errSkipped
			}

			// Run all checks for this category concurrently
			checksGroup := group.NewWithStreaming[string]().WithErrors().
				WithConcurrencyLimiter(checksLimiter)
			// Collect errors in the synchronous callbacks
			var errs error
			for _, check := range category.Checks {
				// copy
				check := check

				// run checks concurrently
				checksGroup.Go(func() (event string, err error) {
					if err := check.IsEnabled(ctx, args); err != nil {
						updateCheckSkipped(i, check.Name, err)

						return "skipped", nil
					}

					// progress.Verbose never writes to output, so we just send check
					// progress to discard.
					var updateOutput strings.Builder
					if err := check.Update(ctx, std.NewFixedOutput(&updateOutput, true), args); err != nil {
						updateCheckFailed(i, check.Name, err)

						check.cachedCheckOutput = updateOutput.String()
						return "error", err
					}

					return "success", nil
				}, func(event string, err error) {
					updateChecksProgress()
					r.logEvent(ctx, []string{"check", category.Name, check.Name}, start, event)

					if err != nil {
						errs = errors.Append(errs, err)
					}
				})
			}
			checksGroup.Wait()

			return errs
		}, func(err error) {
			// record duration
			categoriesDurations[i] = time.Since(start)

			// record if skipped
			if errors.Is(err, errSkipped) {
				categoriesSkipped[i] = true
			}

			// If error'd, status bar has already been set to failed with an error message
			// so we only update if there is no error
			if err == nil {
				updateCategoryCompleted(i)
			}
		})
	}
	categoriesGroup.Wait()

	// Destroy progress and render a complete summary.
	progress.Destroy()
	results := &runAllCategoryChecksResult{
		categories: make(map[string]bool),
	}
	for i, category := range r.Categories {
		results.all = append(results.all, i)
		idx := i + 1

		summaryStr := fmt.Sprintf("%d. %s", idx, category.Name)
		dur, ok := categoriesDurations[i]
		if ok {
			summaryStr = fmt.Sprintf("%s (%ds)", summaryStr, dur/time.Second)
		}

		if _, ok := categoriesSkipped[i]; ok {
			r.Output.WriteSkippedf("%s %s[SKIPPED. Reason: %s]%s", summaryStr,
				output.StyleBold, categoriesSkipped[i], output.StyleReset)
			results.skipped = append(results.skipped, i)
			continue
		}

		// Report if this check is happy or not
		satisfied := category.IsSatisfied()
		results.categories[category.Name] = satisfied
		if satisfied {
			r.Output.WriteSuccessf(summaryStr)
		} else {
			results.failed = append(results.failed, i)
			r.Output.WriteFailuref(summaryStr)

			for _, check := range category.Checks {
				if check.cachedCheckErr != nil {
					// Slightly different formatting for each destination
					var suggestion string
					if r.SuggestOnCheckFailure != nil {
						suggestion = r.SuggestOnCheckFailure(category.Name, check, check.cachedCheckErr)
					}

					// Write the terminal summary to an indented block
					var style = output.CombineStyles(output.StyleBold, output.StyleFailure)
					block := r.Output.Block(output.Linef(output.EmojiFailure, style, check.Name))
					block.Writef("%s\n", check.cachedCheckErr)
					if check.cachedCheckOutput != "" {
						block.Writef("%s\n", check.cachedCheckOutput)
					}
					if suggestion != "" {
						block.WriteLine(output.Styled(output.StyleSuggestion, suggestion))
					}
					block.Close()

					// Build the markdown for the annotation summary
					annotationSummary := fmt.Sprintf("```\n%s\n```", check.cachedCheckErr)

					// Render additional details
					if check.cachedCheckOutput != "" {
						outputMarkdown := fmt.Sprintf("\n\n```term\n%s\n```",
							strings.TrimSpace(check.cachedCheckOutput))

						annotationSummary += outputMarkdown
					}

					if suggestion != "" {
						annotationSummary += fmt.Sprintf("\n\n%s", suggestion)
					}

					if r.GenerateAnnotations {
						generateAnnotation(category.Name, check.Name, annotationSummary)
					}
				}
			}
		}
	}

	if len(results.failed) == 0 {
		if len(results.skipped) == 0 {
			r.Output.Write("")
			r.Output.WriteLine(output.Linef(output.EmojiOk, output.StyleBold, "Everything looks good! Happy hacking!"))
		} else {
			r.Output.Write("")
			r.Output.WriteWarningf("Some checks were skipped.")
		}
	}

	return results
}

func (r *Runner[Args]) presentFailedCategoryWithOptions(ctx context.Context, categoryIdx int, category *Category[Args], args Args, results *runAllCategoryChecksResult) error {
	r.printCategoryHeaderAndDependencies(categoryIdx+1, category)
	fixableCategory := category.HasFixable()

	choices := map[int]string{}
	if fixableCategory {
		choices[1] = "You try fixing all of it for me."
		choices[2] = "I want to fix these manually"
		choices[3] = "Go back"
	} else {
		choices[1] = "I want to fix these manually"
		choices[2] = "Go back"
	}

	choice, err := getChoice(r.Input, r.Output, choices)
	if err != nil {
		return err
	}

	switch choice {
	case 1:
		if fixableCategory {
			r.Output.ClearScreen()
			if !r.fixCategoryAutomatically(ctx, categoryIdx, category, args, results) {
				err = errors.Newf("%s: failed to fix category automatically", category.Name)
			}
		} else {
			err = r.fixCategoryManually(ctx, categoryIdx, category, args)
		}
	case 2:
		err = r.fixCategoryManually(ctx, categoryIdx, category, args)
	case 3:
		return nil
	}
	return err
}

func (r *Runner[Args]) printCategoryHeaderAndDependencies(categoryIdx int, category *Category[Args]) {
	r.Output.WriteLine(output.Linef(output.EmojiLightbulb, output.CombineStyles(output.StyleSearchQuery, output.StyleBold), "%d. %s", categoryIdx, category.Name))
	r.Output.Write("")
	r.Output.Write("Checks:")

	for i, dep := range category.Checks {
		idx := i + 1
		if dep.IsSatisfied() {
			r.Output.WriteSuccessf("%d. %s", idx, dep.Name)
		} else {
			if dep.cachedCheckErr != nil {
				r.Output.WriteFailuref("%d. %s: %s", idx, dep.Name, dep.cachedCheckErr)
			} else {
				r.Output.WriteFailuref("%d. %s: %s", idx, dep.Name, "check failed")
			}
		}
	}
}

func (r *Runner[Args]) fixCategoryManually(ctx context.Context, categoryIdx int, category *Category[Args], args Args) error {
	for {
		toFix := []int{}

		for i, dep := range category.Checks {
			if dep.IsSatisfied() {
				continue
			}

			toFix = append(toFix, i)
		}

		if len(toFix) == 0 {
			break
		}

		var idx int

		if len(toFix) == 1 {
			idx = toFix[0]
		} else {
			r.Output.WriteNoticef("Which one do you want to fix?")
			var err error
			idx, err = getNumberOutOf(r.Input, r.Output, toFix)
			if err != nil {
				if err == io.EOF {
					return nil
				}
				return err
			}
		}

		check := category.Checks[idx]

		r.Output.WriteLine(output.Linef(output.EmojiFailure, output.CombineStyles(output.StyleWarning, output.StyleBold), "%s", check.Name))
		r.Output.Write("")

		if check.cachedCheckErr != nil {
			r.Output.WriteLine(output.Styledf(output.StyleBold, "Check encountered the following error:\n\n%s%s\n", output.StyleReset, check.cachedCheckErr))
		}

		if check.Description == "" {
			return errors.Newf("No description available for manual fix - good luck!")
		}

		r.Output.WriteLine(output.Styled(output.StyleBold, "How to fix:"))

		r.Output.WriteMarkdown(check.Description)

		// Wait for user to finish
		r.Output.Promptf("Hit 'Return' or 'Enter' when you are done.")
		waitForReturn(r.Input)

		// Check statuses
		r.Output.WriteLine(output.Styled(output.StylePending, "Running check..."))
		if err := check.Update(ctx, r.Output, args); err != nil {
			r.Output.WriteWarningf("Check %q still not satisfied", check.Name)
			return err
		}

		// Print summary again
		r.printCategoryHeaderAndDependencies(categoryIdx, category)
	}

	return nil
}

func (r *Runner[Args]) fixCategoryAutomatically(ctx context.Context, categoryIdx int, category *Category[Args], args Args, results *runAllCategoryChecksResult) (ok bool) {
	// Best to be verbose when fixing, in case something goes wrong
	r.Output.SetVerbose()
	defer r.Output.UnsetVerbose()

	start := time.Now()
	r.Output.WriteLine(output.Styledf(output.StylePending, "Trying my hardest to fix %q automatically...", category.Name))

	// Make sure to call this with a final message before returning!
	complete := func(emoji string, style output.Style, fmtStr string, args ...any) {
		r.Output.WriteLine(output.Linef(emoji, output.CombineStyles(style, output.StyleBold),
			"%d. %s - "+fmtStr, append([]any{categoryIdx, category.Name}, args...)...))
	}

	if err := category.CheckEnabled(ctx, args); err != nil {
		complete(output.EmojiQuestionMark, output.StyleGrey, "Skipped: %s", err.Error())
		return true
	}

	// If nothing in this category is fixable, we are done
	if !category.HasFixable() {
		complete(output.EmojiFailure, output.StyleFailure, "Cannot be fixed automatically.")
		return false
	}

	// Only run if dependents are fixed
	var unmetDependencies []string
	for _, d := range category.DependsOn {
		if met, exists := results.categories[d]; !exists {
			complete(output.EmojiFailure, output.StyleFailure, "Required check category %q not found", d)
			return false
		} else if !met {
			unmetDependencies = append(unmetDependencies, fmt.Sprintf("%q", d))
		}
	}
	if len(unmetDependencies) > 0 {
		complete(output.EmojiFailure, output.StyleFailure, "Required dependencies %s not met.", strings.Join(unmetDependencies, ", "))
		return false
	}

	// now go through the real dependencies
	for _, c := range category.Checks {
		logEvent := func(event string) {
			r.logEvent(ctx, []string{"fix", category.Name, c.Name}, start, event)
		}

		// If category is fixed, we are good to go
		if c.IsSatisfied() {
			continue
		}

		// Skip
		if err := c.IsEnabled(ctx, args); err != nil {
			r.Output.WriteLine(output.Linef(output.EmojiQuestionMark, output.CombineStyles(output.StyleGrey, output.StyleBold),
				"%q skipped: %s", c.Name, err.Error()))
			logEvent("skipped")
			continue
		}

		// Otherwise, check if this is fixable at all
		if c.Fix == nil {
			r.Output.WriteLine(output.Linef(output.EmojiShrug, output.CombineStyles(output.StyleWarning, output.StyleBold),
				"%q cannot be fixed automatically.", c.Name))
			logEvent("unfixable")
			continue
		}

		// Attempt fix. Get new args because things might have changed due to another
		// fix being run.
		r.Output.VerboseLine(output.Linef(output.EmojiAsterisk, output.StylePending,
			"Fixing %q...", c.Name))
		err := c.Fix(ctx, IO{
			Input:  r.Input,
			Output: r.Output,
		}, args)
		if err != nil {
			r.Output.WriteLine(output.Linef(output.EmojiWarning, output.CombineStyles(output.StyleFailure, output.StyleBold),
				"Failed to fix %q: %s", c.Name, err.Error()))
			logEvent("failed")
			continue
		}

		// Check if the fix worked, or just don't check
		if !r.RunPostFixChecks {
			err = nil
			c.cachedCheckErr = nil
			c.cachedCheckOutput = ""
		} else {
			err = c.Update(ctx, r.Output, args)
		}

		if err != nil {
			r.Output.WriteLine(output.Styledf(output.CombineStyles(output.StyleWarning, output.StyleBold),
				"Check %q still failing: %s", c.Name, err.Error()))
			logEvent("unfixed")
		} else {
			r.Output.WriteLine(output.Styledf(output.CombineStyles(output.StyleSuccess, output.StyleBold),
				"Check %q is satisfied now!", c.Name))
			logEvent("success")
		}
	}

	ok = category.IsSatisfied()
	if ok {
		complete(output.EmojiSuccess, output.StyleSuccess, "Done!")
	} else {
		complete(output.EmojiFailure, output.StyleFailure, "Some checks are still not satisfied")
	}

	return
}

// logEvent logs an event if AnalyticsCategory is set.
func (r *Runner[Args]) logEvent(ctx context.Context, labels []string, startedAt time.Time, events ...string) {
	if r.AnalyticsCategory != "" {
		analytics.LogEvent(ctx, r.AnalyticsCategory, labels, startedAt, events...)
	}
}
