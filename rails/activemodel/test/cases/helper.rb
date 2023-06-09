# frozen_string_literal: true

require "active_support/testing/strict_warnings"
require "active_model"

# Show backtraces for deprecated behavior for quicker cleanup.
ActiveModel.deprecator.debug = true

# Disable available locale checks to avoid warnings running the test suite.
I18n.enforce_available_locales = false

require "active_support/testing/autorun"
require "active_support/testing/method_call_assertions"
require "active_support/core_ext/integer/time"

class ActiveModel::TestCase < ActiveSupport::TestCase
  include ActiveSupport::Testing::MethodCallAssertions

  class AssertionlessTest < StandardError; end

  def after_teardown
    super

    raise AssertionlessTest, "No assertions made." if passed? && assertions.zero?
  end

  private
    # Skips the current run on JRuby using Minitest::Assertions#skip
    def jruby_skip(message = "")
      skip message if defined?(JRUBY_VERSION)
    end
end

require_relative "../../../tools/test_common"
