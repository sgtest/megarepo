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

### Deprecations

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
