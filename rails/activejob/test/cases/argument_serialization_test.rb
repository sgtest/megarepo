# frozen_string_literal: true

require "bigdecimal"
require "helper"
require "active_job/arguments"
require "models/person"
require "active_support/core_ext/hash/indifferent_access"
require "active_support/core_ext/integer/time"
require "active_support/duration"
require "jobs/kwargs_job"
require "jobs/arguments_round_trip_job"
require "support/stubs/strong_parameters"

class ArgumentSerializationTest < ActiveSupport::TestCase
  module ModuleArgument
    class ClassArgument; end
  end

  class ClassArgument; end

  class MyClassWithPermitted
    def self.permitted?
    end
  end

  setup do
    @person = Person.find("5")
  end

  [ nil, 1, 1.0, 1_000_000_000_000_000_000_000,
    "a", true, false,
    :a,
    1.day,
    Date.new(2001, 2, 3),
    Time.new(2002, 10, 31, 2, 2, 2.123456789r, "+02:00"),
    DateTime.new(2001, 2, 3, 4, 5, 6.123456r, "+03:00"),
    ActiveSupport::TimeWithZone.new(Time.utc(1999, 12, 31, 23, 59, "59.123456789".to_r), ActiveSupport::TimeZone["UTC"]),
    [ 1, "a" ],
    { "a" => 1 },
    ModuleArgument,
    ModuleArgument::ClassArgument,
    ClassArgument,
    1..,
    1...,
    1..5,
    1...5,
    "a".."z",
    "A".."Z",
    Date.new(2001, 2, 3)..,
    10.days.ago..Date.today,
    Time.new(2002, 10, 31, 2, 2, 2.123456789r, "+02:00")..,
    10.hours.ago..Time.current,
    DateTime.new(2001, 2, 3, 4, 5, 6.123456r, "+03:00")..,
    (DateTime.current - 4.weeks)..DateTime.current,
    ActiveSupport::TimeWithZone.new(Time.utc(1999, 12, 31, 23, 59, "59.123456789".to_r), ActiveSupport::TimeZone["UTC"])..,
  ].each do |arg|
    test "serializes #{arg.class} - #{arg.inspect} verbatim" do
      assert_arguments_unchanged arg
    end
  end

  test "dangerously treats BigDecimal arguments as primitives not requiring serialization by default" do
    assert_deprecated(<<~MSG.chomp, ActiveJob.deprecator) do
      Primitive serialization of BigDecimal job arguments is deprecated as it may serialize via .to_s using certain queue adapters.
      Enable config.active_job.use_big_decimal_serializer to use BigDecimalSerializer instead, which will be mandatory in Rails 7.2.

      Note that if you application has multiple replicas, you should only enable this setting after successfully deploying your app to Rails 7.1 first.
      This will ensure that during your deployment all replicas are capable of deserializing arguments serialized with BigDecimalSerializer.
    MSG
      assert_equal(
        BigDecimal(5),
        *ActiveJob::Arguments.deserialize(ActiveJob::Arguments.serialize([BigDecimal(5)])),
      )
    end
  end

  test "safely serializes BigDecimal arguments if configured to use_big_decimal_serializer" do
    # BigDecimal(5) example should be moved back up into array above in Rails 7.2
    with_big_decimal_serializer do
      assert_arguments_unchanged BigDecimal(5)
    end
  end

  [ Object.new, Person.find("5").to_gid, Class.new ].each do |arg|
    test "does not serialize #{arg.class}" do
      assert_raises ActiveJob::SerializationError do
        ActiveJob::Arguments.serialize [ arg ]
      end

      assert_raises ActiveJob::DeserializationError do
        ActiveJob::Arguments.deserialize [ arg ]
      end
    end
  end

  test "should convert records to Global IDs" do
    assert_arguments_roundtrip [@person]
  end

  test "should keep Global IDs strings as they are" do
    assert_arguments_roundtrip [@person.to_gid.to_s]
  end

  test "should dive deep into arrays and hashes" do
    assert_arguments_roundtrip [3, [@person]]
    assert_arguments_roundtrip [{ "a" => @person }]
  end

  test "should maintain string and symbol keys" do
    assert_arguments_roundtrip([a: 1, "b" => 2])
  end

  test "serialize a ActionController::Parameters" do
    parameters = Parameters.new(a: 1)

    assert_equal(
      { "a" => 1, "_aj_hash_with_indifferent_access" => true },
      ActiveJob::Arguments.serialize([parameters]).first
    )
  end

  # Regression test to #48561
  test "serialize a class with permitted? defined" do
    assert_arguments_unchanged MyClassWithPermitted
  end

  test "serialize a hash" do
    symbol_key = { a: 1 }
    string_key = { "a" => 1 }
    indifferent_access = { a: 1 }.with_indifferent_access

    assert_equal(
      { "a" => 1, "_aj_symbol_keys" => ["a"] },
      ActiveJob::Arguments.serialize([symbol_key]).first
    )
    assert_equal(
      { "a" => 1, "_aj_symbol_keys" => [] },
      ActiveJob::Arguments.serialize([string_key]).first
    )
    assert_equal(
      { "a" => 1, "_aj_hash_with_indifferent_access" => true },
      ActiveJob::Arguments.serialize([indifferent_access]).first
    )
  end

  test "deserialize a hash" do
    symbol_key = { "a" => 1, "_aj_symbol_keys" => ["a"] }
    string_key = { "a" => 1, "_aj_symbol_keys" => [] }
    another_string_key = { "a" => 1 }
    indifferent_access = { "a" => 1, "_aj_hash_with_indifferent_access" => true }
    indifferent_access_symbol_key = symbol_key.with_indifferent_access

    assert_equal(
      { a: 1 },
      ActiveJob::Arguments.deserialize([symbol_key]).first
    )
    assert_equal(
      { "a" => 1 },
      ActiveJob::Arguments.deserialize([string_key]).first
    )
    assert_equal(
      { "a" => 1 },
      ActiveJob::Arguments.deserialize([another_string_key]).first
    )
    assert_equal(
      { "a" => 1 },
      ActiveJob::Arguments.deserialize([indifferent_access]).first
    )
    assert_equal(
      { a: 1 },
      ActiveJob::Arguments.deserialize([indifferent_access_symbol_key]).first
    )
  end

  test "should maintain hash with indifferent access" do
    symbol_key = { a: 1 }
    string_key = { "a" => 1 }
    indifferent_access = { a: 1 }.with_indifferent_access

    assert_not_instance_of ActiveSupport::HashWithIndifferentAccess, perform_round_trip([symbol_key]).first
    assert_not_instance_of ActiveSupport::HashWithIndifferentAccess, perform_round_trip([string_key]).first
    assert_instance_of ActiveSupport::HashWithIndifferentAccess, perform_round_trip([indifferent_access]).first
  end

  test "should maintain time with zone" do
    Time.use_zone "Alaska" do
      time_with_zone = Time.new(2002, 10, 31, 2, 2, 2).in_time_zone
      assert_instance_of ActiveSupport::TimeWithZone, perform_round_trip([time_with_zone]).first
      assert_arguments_unchanged time_with_zone
    end
  end

  test "should maintain a functional duration" do
    duration = perform_round_trip([1.year]).first
    assert_kind_of Hash, duration.parts
    assert_equal 2.years, duration + 1.year
  end

  test "should disallow non-string/symbol hash keys" do
    assert_raises ActiveJob::SerializationError do
      ActiveJob::Arguments.serialize [ { 1 => 2 } ]
    end

    assert_raises ActiveJob::SerializationError do
      ActiveJob::Arguments.serialize [ { a: [{ 2 => 3 }] } ]
    end
  end

  test "should not allow reserved hash keys" do
    ["_aj_globalid", :_aj_globalid,
     "_aj_symbol_keys", :_aj_symbol_keys,
     "_aj_hash_with_indifferent_access", :_aj_hash_with_indifferent_access,
     "_aj_serialized", :_aj_serialized].each do |key|
      assert_raises ActiveJob::SerializationError do
        ActiveJob::Arguments.serialize [key => 1]
      end
    end
  end

  test "should not allow non-primitive objects" do
    assert_raises ActiveJob::SerializationError do
      ActiveJob::Arguments.serialize [Object.new]
    end

    assert_raises ActiveJob::SerializationError do
      ActiveJob::Arguments.serialize [1, [Object.new]]
    end
  end

  test "allows for keyword arguments" do
    KwargsJob.perform_now(argument: 2)

    assert_equal "Job with argument: 2", JobBuffer.last_value
  end

  test "raises a friendly SerializationError for records without ids" do
    err = assert_raises ActiveJob::SerializationError do
      ActiveJob::Arguments.serialize [Person.new(nil)]
    end
    assert_match "Unable to serialize Person without an id.", err.message
  end

  private
    def assert_arguments_unchanged(*args)
      assert_arguments_roundtrip args
    end

    def assert_arguments_roundtrip(args)
      assert_equal args, perform_round_trip(args)
    end

    def perform_round_trip(args)
      ArgumentsRoundTripJob.perform_later(*args) # Actually performed inline

      JobBuffer.last_value
    end

    def with_big_decimal_serializer(temporary = true)
      original = ActiveJob.use_big_decimal_serializer
      ActiveJob.use_big_decimal_serializer = temporary
      yield
    ensure
      ActiveJob.use_big_decimal_serializer = original
    end
end
