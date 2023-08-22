# frozen_string_literal: true

require "isolation/abstract_unit"
require "console_helpers"

class ConsoleTest < ActiveSupport::TestCase
  include ActiveSupport::Testing::Isolation

  def setup
    build_app
  end

  def teardown
    teardown_app
  end

  def load_environment(sandbox = false)
    require "#{rails_root}/config/environment"
    Rails.application.sandbox = sandbox
    Rails.application.load_console
  end

  def irb_context
    Object.new.extend(Rails::ConsoleMethods)
  end

  def test_app_method_should_return_integration_session
    TestHelpers::Rack.remove_method :app
    load_environment
    console_session = irb_context.app
    assert_instance_of ActionDispatch::Integration::Session, console_session
  end

  def test_app_can_access_path_helper_method
    app_file "config/routes.rb", <<-RUBY
      Rails.application.routes.draw do
        get 'foo', to: 'foo#index'
      end
    RUBY

    load_environment
    console_session = irb_context.app
    assert_equal "/foo", console_session.foo_path
  end

  def test_new_session_should_return_integration_session
    load_environment
    session = irb_context.new_session
    assert_instance_of ActionDispatch::Integration::Session, session
  end

  def test_reload_should_fire_preparation_and_cleanup_callbacks
    load_environment
    a = b = c = nil

    # TODO: These should be defined on the initializer
    ActiveSupport::Reloader.to_complete { a = b = c = 1 }
    ActiveSupport::Reloader.to_complete { b = c = 2 }
    ActiveSupport::Reloader.to_prepare { c = 3 }

    irb_context.reload!(false)

    assert_equal 1, a
    assert_equal 2, b
    assert_equal 3, c
  end

  def test_reload_should_reload_constants
    app_file "app/models/user.rb", <<-MODEL
      class User
        attr_accessor :name
      end
    MODEL

    load_environment
    assert_respond_to User.new, :name

    app_file "app/models/user.rb", <<-MODEL
      class User
        attr_accessor :name, :age
      end
    MODEL

    assert_not_respond_to User.new, :age
    irb_context.reload!(false)
    assert_respond_to User.new, :age
  end

  def test_access_to_helpers
    load_environment
    helper = irb_context.helper
    assert_not_nil helper
    assert_instance_of ActionView::Base, helper
    assert_equal "Once upon a time in a world...",
      helper.truncate("Once upon a time in a world far far away")
  end
end

class FullStackConsoleTest < ActiveSupport::TestCase
  include ConsoleHelpers

  def setup
    skip "PTY unavailable" unless available_pty?

    build_app
    app_file "app/models/post.rb", <<-CODE
      class Post < ActiveRecord::Base
      end
    CODE
    system "#{app_path}/bin/rails runner 'Post.connection.create_table :posts'"

    @primary, @replica = PTY.open
  end

  def teardown
    teardown_app
  end

  def write_prompt(command, expected_output = nil)
    @primary.puts command
    assert_output command, @primary
    assert_output expected_output, @primary if expected_output
    assert_output "> ", @primary
  end

  def spawn_console(options, wait_for_prompt: true)
    pid = Process.spawn(
      { "TERM" => "dumb" },
      "#{app_path}/bin/rails console #{options}",
      in: @replica, out: @replica, err: @replica
    )

    if wait_for_prompt
      assert_output "> ", @primary, 30
    end

    pid
  end

  def test_sandbox
    options = "--sandbox -- --nocolorize"
    spawn_console(options)

    write_prompt "Post.count", "=> 0"
    write_prompt "Post.create"
    write_prompt "Post.count", "=> 1"
    @primary.puts "quit"

    spawn_console(options)

    write_prompt "Post.count", "=> 0"
    write_prompt "Post.transaction { Post.create; raise }"
    write_prompt "Post.count", "=> 0"
    @primary.puts "quit"
  end

  def test_sandbox_when_sandbox_is_disabled
    add_to_config <<-RUBY
      config.disable_sandbox = true
    RUBY

    output = `#{app_path}/bin/rails console --sandbox`

    assert_includes output, "sandbox mode is disabled"
    assert_equal 1, $?.exitstatus
  end

  def test_sandbox_by_default
    add_to_config <<-RUBY
      config.sandbox_by_default = true
    RUBY

    options = "-e production -- --verbose --nocolorize"
    spawn_console(options)

    write_prompt "puts Rails.application.sandbox", "puts Rails.application.sandbox\r\ntrue"
    @primary.puts "quit"
  end

  def test_sandbox_by_default_with_no_sandbox
    add_to_config <<-RUBY
      config.sandbox_by_default = true
    RUBY

    options = "-e production --no-sandbox -- --verbose --nocolorize"
    spawn_console(options)

    write_prompt "puts Rails.application.sandbox", "puts Rails.application.sandbox\r\nfalse"
    @primary.puts "quit"
  end

  def test_sandbox_by_default_with_development_environment
    add_to_config <<-RUBY
      config.sandbox_by_default = true
    RUBY

    options = "-- --verbose --nocolorize"
    spawn_console(options)

    write_prompt "puts Rails.application.sandbox", "puts Rails.application.sandbox\r\nfalse"
    @primary.puts "quit"
  end

  def test_environment_option_and_irb_option
    options = "-e test -- --verbose --nocolorize"
    spawn_console(options)

    write_prompt "a = 1", "a = 1"
    write_prompt "puts Rails.env", "puts Rails.env\r\ntest"
    @primary.puts "quit"
  end
end
