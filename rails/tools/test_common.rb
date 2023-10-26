# frozen_string_literal: true

if ENV["BUILDKITE"]
  require "minitest-ci"
  ENV.delete("CI") # CI has affect on the applications, and we don't want it applied to the apps.

  Minitest::Ci.report_dir = File.join(__dir__, "../test-reports/#{ENV['BUILDKITE_JOB_ID']}")
end

if ENV["CI"]
  require File.join(__dir__, "../activesupport/lib/active_support/testing/no_skip")
  ActiveSupport::TestCase.include(ActiveSupport::Testing::NoSkip)
end
