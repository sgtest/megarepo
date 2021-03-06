# frozen_string_literal: true

require "helper"
require "jobs/gid_job"
require "jobs/hello_job"
require "models/person"
require "json"

class JobSerializationTest < ActiveSupport::TestCase
  setup do
    JobBuffer.clear
    @person = Person.find(5)
  end

  test "serialize job with gid" do
    GidJob.perform_later @person
    assert_equal "Person with ID: 5", JobBuffer.last_value
  end

  test "serialize includes current locale" do
    assert_equal "en", HelloJob.new.serialize["locale"]
  end

  test "serialize and deserialize are symmetric" do
    # Ensure `enqueued_at` does not change between serializations
    freeze_time

    # Round trip a job in memory only
    h1 = HelloJob.new("Rafael")
    h2 = HelloJob.deserialize(h1.serialize)
    assert_equal h1.serialize, h2.serialize

    # Now verify it's identical to a JSON round trip.
    # We don't want any non-native JSON elements in the job hash,
    # like symbols.
    payload = JSON.dump(h2.serialize)
    h3 = HelloJob.deserialize(JSON.load(payload))
    assert_equal h2.serialize, h3.serialize
  end

  test "deserialize sets locale" do
    job = HelloJob.new
    job.deserialize "locale" => "es"
    assert_equal "es", job.locale
  end

  test "deserialize sets default locale" do
    job = HelloJob.new
    job.deserialize({})
    assert_equal "en", job.locale
  end

  test "serialize stores provider_job_id" do
    job = HelloJob.new
    assert_nil job.serialize["provider_job_id"]

    job.provider_job_id = "some value set by adapter"
    assert_equal job.provider_job_id, job.serialize["provider_job_id"]
  end

  test "serialize stores the current timezone" do
    Time.use_zone "Hawaii" do
      job = HelloJob.new
      assert_equal "Hawaii", job.serialize["timezone"]
    end
  end

  test "serializes enqueued_at with full precision" do
    freeze_time

    serialized = HelloJob.new.serialize
    assert_kind_of String, serialized["enqueued_at"]

    enqueued_at = HelloJob.deserialize(serialized).enqueued_at
    assert_equal Time.now.utc, Time.iso8601(enqueued_at)
  end
end
