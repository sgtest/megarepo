# frozen_string_literal: true

require "generators/generators_test_helper"
require "rails/generators/rails/db/system/change/change_generator"

module Rails
  module Generators
    module Db
      module System
        class ChangeGeneratorTest < Rails::Generators::TestCase
          include GeneratorsTestHelper

          setup do
            copy_gemfile <<~ENTRY
              # Use sqlite3 as the database for Active Record
              gem "sqlite3"
            ENTRY

            copy_dockerfile
          end

          test "change to invalid database" do
            output = capture(:stderr) do
              run_generator ["--to", "invalid-db"]
            end

            assert_match <<~MSG.squish, output
              Invalid value for --to option.
              Supported preconfigurations are:
              mysql, trilogy, postgresql, sqlite3,
              oracle, sqlserver, jdbcmysql,
              jdbcsqlite3, jdbcpostgresql, jdbc.
            MSG
          end

          test "change to postgresql" do
            run_generator ["--to", "postgresql"]

            assert_file("config/database.yml") do |content|
              assert_match "adapter: postgresql", content
              assert_match "database: tmp_production", content
            end

            assert_file("Gemfile") do |content|
              assert_match "# Use pg as the database for Active Record", content
              assert_match 'gem "pg", "~> 1.1"', content
            end

            assert_file("Dockerfile") do |content|
              assert_match "build-essential git libpq-dev", content
              assert_match "curl libvips postgresql-client", content
            end
          end

          test "change to mysql" do
            run_generator ["--to", "mysql"]

            assert_file("config/database.yml") do |content|
              assert_match "adapter: mysql2", content
              assert_match "database: tmp_production", content
            end

            assert_file("Gemfile") do |content|
              assert_match "# Use mysql2 as the database for Active Record", content
              assert_match 'gem "mysql2", "~> 0.5"', content
            end

            assert_file("Dockerfile") do |content|
              assert_match "build-essential default-libmysqlclient-dev git", content
              assert_match "curl default-mysql-client libvips", content
            end
          end

          test "change to sqlite3" do
            run_generator ["--to", "sqlite3"]

            assert_file("config/database.yml") do |content|
              assert_match "adapter: sqlite3", content
              assert_match "storage/development.sqlite3", content
            end

            assert_file("Gemfile") do |content|
              assert_match "# Use sqlite3 as the database for Active Record", content
              assert_match 'gem "sqlite3", "~> 1.4"', content
            end

            assert_file("Dockerfile") do |content|
              assert_match "build-essential git libvips", content
              assert_match "curl libsqlite3-0 libvips", content
            end
          end

          test "change from versioned gem to other versioned gem" do
            run_generator ["--to", "sqlite3"]
            run_generator ["--to", "mysql", "--force"]

            assert_file("config/database.yml") do |content|
              assert_match "adapter: mysql2", content
              assert_match "database: tmp_production", content
            end

            assert_file("Gemfile") do |content|
              assert_match "# Use mysql2 as the database for Active Record", content
              assert_match 'gem "mysql2", "~> 0.5"', content
            end
          end
        end
      end
    end
  end
end
