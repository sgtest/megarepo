**DO NOT READ THIS FILE ON GITHUB, GUIDES ARE PUBLISHED ON https://guides.rubyonrails.org.**

Ruby on Rails 7.2 Release Notes
===============================

Highlights in Rails 7.2:

--------------------------------------------------------------------------------

Upgrading to Rails 7.2
----------------------

If you're upgrading an existing application, it's a great idea to have good test
coverage before going in. You should also first upgrade to Rails 7.1 in case you
haven't and make sure your application still runs as expected before attempting
an update to Rails 7.2. A list of things to watch out for when upgrading is
available in the
[Upgrading Ruby on Rails](upgrading_ruby_on_rails.html#upgrading-from-rails-7-1-to-rails-7-2)
guide.

Major Features
--------------

Railties
--------

Please refer to the [Changelog][railties] for detailed changes.

### Removals

### Deprecations

### Notable changes

Action Cable
------------

Please refer to the [Changelog][action-cable] for detailed changes.

### Removals

### Deprecations

### Notable changes

Action Pack
-----------

Please refer to the [Changelog][action-pack] for detailed changes.

### Removals

*   Remove deprecated constant `ActionDispatch::IllegalStateError`.

*   Remove deprecated constant `AbstractController::Helpers::MissingHelperError`.

*   Remove deprecated comparison between `ActionController::Parameters` and `Hash`.

*   Remove deprecated `Rails.application.config.action_dispatch.return_only_request_media_type_on_content_type`.

*   Remove deprecated `speaker`, `vibrate`, and `vr` permissions policy directives.

*   Remove deprecated support to set `Rails.application.config.action_dispatch.show_exceptions` to `true` and `false`.

### Deprecations

*   Deprecate `Rails.application.config.action_controller.allow_deprecated_parameters_hash_equality`.

### Notable changes

Action View
-----------

Please refer to the [Changelog][action-view] for detailed changes.

### Removals

*   Remove deprecated `@rails/ujs` in favor of `Turbo`.

### Deprecations

*  Deprecate passing content to void elements when using `tag.br` type tag builders.

### Notable changes

Action Mailer
-------------

Please refer to the [Changelog][action-mailer] for detailed changes.

### Removals

*   Remove deprecated `config.action_mailer.preview_path`.

*   Remove deprecated params via `:args` for `assert_enqueued_email_with`.

### Deprecations

### Notable changes

Active Record
-------------

Please refer to the [Changelog][active-record] for detailed changes.

### Removals

*   Remove deprecated `Rails.application.config.active_record.suppress_multiple_database_warning`.

*   Remove deprecated support to call `alias_attribute` with non-existent attribute names.

*   Remove deprecated `name` argument from `ActiveRecord::Base.remove_connection`.

*   Remove deprecated `ActiveRecord::Base.clear_active_connections!`.

*   Remove deprecated `ActiveRecord::Base.clear_reloadable_connections!`.

*   Remove deprecated `ActiveRecord::Base.clear_all_connections!`.

*   Remove deprecated `ActiveRecord::Base.flush_idle_connections!`.

*   Remove deprecated `ActiveRecord::ActiveJobRequiredError`.

*   Remove deprecated support to define `explain` in the connection adapter with 2 arguments.

*   Remove deprecated `ActiveRecord::LogSubscriber.runtime` method.

*   Remove deprecated `ActiveRecord::LogSubscriber.runtime=` method.

*   Remove deprecated `ActiveRecord::LogSubscriber.reset_runtime` method.

*   Remove deprecated `ActiveRecord::Migration.check_pending` method.

*   Remove deprecated support to passing `SchemaMigration` and `InternalMetadata` classes as arguments to
    `ActiveRecord::MigrationContext`.

*   Remove deprecated behavior to support referring to a singular association by its plural name.

*   Remove deprecated `TestFixtures.fixture_path`.

*   Remove deprecated support to `ActiveRecord::Base#read_attribute(:id)` to return the custom primary key value.

*   Remove deprecated support to passing coder and class as second argument to `serialize`.

*   Remove deprecated `#all_foreign_keys_valid?` from database adapters.

*   Remove deprecated `ActiveRecord::ConnectionAdapters::SchemaCache.load_from`.

*   Remove deprecated `ActiveRecord::ConnectionAdapters::SchemaCache#data_sources`.

*   Remove deprecated `#all_connection_pools`.

*   Remove deprecated support to apply `#connection_pool_list`, `#active_connections?`, `#clear_active_connections!`,
    `#clear_reloadable_connections!`, `#clear_all_connections!` and `#flush_idle_connections!` to the connections pools
    for the current role when the `role` argument isn't provided.

*   Remove deprecated `ActiveRecord::ConnectionAdapters::ConnectionPool#connection_klass`.

*   Remove deprecated `#quote_bound_value`.

*   Remove deprecated support to quote `ActiveSupport::Duration`.

*   Remove deprecated support to pass `deferrable: true` to `add_foreign_key`.

*   Remove deprecated support to pass `rewhere` to `ActiveRecord::Relation#merge`.

*   Remove deprecated behavior that would rollback a transaction block when exited using `return`, `break` or `throw`.

### Deprecations

*   Deprecate `Rails.application.config.active_record.allow_deprecated_singular_associations_name`

*   Deprecate `Rails.application.config.active_record.commit_transaction_on_non_local_return`

### Notable changes

Active Storage
--------------

Please refer to the [Changelog][active-storage] for detailed changes.

### Removals

### Deprecations

### Notable changes

Active Model
------------

Please refer to the [Changelog][active-model] for detailed changes.

### Removals

### Deprecations

### Notable changes

Active Support
--------------

Please refer to the [Changelog][active-support] for detailed changes.

### Removals

### Deprecations

### Notable changes

Active Job
----------

Please refer to the [Changelog][active-job] for detailed changes.

### Removals

*   Remove deprecated primitive serializer for `BigDecimal` arguments.

*   Remove deprecated support to set numeric values to `scheduled_at` attribute.

*   Remove deprecated `:exponentially_longer` value for the `:wait` in `retry_on`.

### Deprecations

*   Deprecate `Rails.application.config.active_job.use_big_decimal_serialize`.

### Notable changes

Action Text
----------

Please refer to the [Changelog][action-text] for detailed changes.

### Removals

### Deprecations

### Notable changes

Action Mailbox
----------

Please refer to the [Changelog][action-mailbox] for detailed changes.

### Removals

### Deprecations

### Notable changes

Ruby on Rails Guides
--------------------

Please refer to the [Changelog][guides] for detailed changes.

### Notable changes

Credits
-------

See the
[full list of contributors to Rails](https://contributors.rubyonrails.org/)
for the many people who spent many hours making Rails, the stable and robust
framework it is. Kudos to all of them.

[railties]:       https://github.com/rails/rails/blob/main/railties/CHANGELOG.md
[action-pack]:    https://github.com/rails/rails/blob/main/actionpack/CHANGELOG.md
[action-view]:    https://github.com/rails/rails/blob/main/actionview/CHANGELOG.md
[action-mailer]:  https://github.com/rails/rails/blob/main/actionmailer/CHANGELOG.md
[action-cable]:   https://github.com/rails/rails/blob/main/actioncable/CHANGELOG.md
[active-record]:  https://github.com/rails/rails/blob/main/activerecord/CHANGELOG.md
[active-storage]: https://github.com/rails/rails/blob/main/activestorage/CHANGELOG.md
[active-model]:   https://github.com/rails/rails/blob/main/activemodel/CHANGELOG.md
[active-support]: https://github.com/rails/rails/blob/main/activesupport/CHANGELOG.md
[active-job]:     https://github.com/rails/rails/blob/main/activejob/CHANGELOG.md
[action-text]:    https://github.com/rails/rails/blob/main/actiontext/CHANGELOG.md
[action-mailbox]: https://github.com/rails/rails/blob/main/actionmailbox/CHANGELOG.md
[guides]:         https://github.com/rails/rails/blob/main/guides/CHANGELOG.md
