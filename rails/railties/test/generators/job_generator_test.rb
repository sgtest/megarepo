# frozen_string_literal: true

require "generators/generators_test_helper"
require "rails/generators/job/job_generator"

class JobGeneratorTest < Rails::Generators::TestCase
  include GeneratorsTestHelper

  def test_job_skeleton_is_created
    run_generator ["refresh_counters"]
    assert_file "app/jobs/refresh_counters_job.rb" do |job|
      assert_match(/class RefreshCountersJob < ApplicationJob/, job)
    end
  end

  def test_job_queue_param
    run_generator ["refresh_counters", "--queue", "important"]
    assert_file "app/jobs/refresh_counters_job.rb" do |job|
      assert_match(/class RefreshCountersJob < ApplicationJob/, job)
      assert_match(/queue_as :important/, job)
    end
  end

  def test_job_parent_param
    run_generator ["refresh_counters", "--parent", "awesome_job"]
    assert_file "app/jobs/refresh_counters_job.rb" do |job|
      assert_match(/class RefreshCountersJob < AwesomeJob/, job)
    end
  end

  def test_job_namespace
    run_generator ["admin/refresh_counters", "--queue", "admin"]
    assert_file "app/jobs/admin/refresh_counters_job.rb" do |job|
      assert_match(/class Admin::RefreshCountersJob < ApplicationJob/, job)
      assert_match(/queue_as :admin/, job)
    end
  end

  def test_application_job_skeleton_is_created
    run_generator ["refresh_counters"]
    assert_file "app/jobs/application_job.rb" do |job|
      assert_match(/class ApplicationJob < ActiveJob::Base/, job)
    end
  end

  def test_job_suffix_is_not_duplicated
    run_generator ["notifier_job"]

    assert_no_file "app/jobs/notifier_job_job.rb"
    assert_file "app/jobs/notifier_job.rb"

    assert_no_file "test/jobs/notifier_job_job_test.rb"
    assert_file "test/jobs/notifier_job_test.rb"
  end
end
