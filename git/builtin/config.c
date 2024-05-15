#include "builtin.h"
#include "abspath.h"
#include "config.h"
#include "color.h"
#include "editor.h"
#include "environment.h"
#include "repository.h"
#include "gettext.h"
#include "ident.h"
#include "parse-options.h"
#include "urlmatch.h"
#include "path.h"
#include "quote.h"
#include "setup.h"
#include "strbuf.h"
#include "worktree.h"

static const char *const builtin_config_usage[] = {
	N_("git config list [<file-option>] [<display-option>] [--includes]"),
	N_("git config get [<file-option>] [<display-option>] [--includes] [--all] [--regexp=<regexp>] [--value=<value>] [--fixed-value] [--default=<default>] <name>"),
	N_("git config set [<file-option>] [--type=<type>] [--all] [--value=<value>] [--fixed-value] <name> <value>"),
	N_("git config unset [<file-option>] [--all] [--value=<value>] [--fixed-value] <name> <value>"),
	N_("git config rename-section [<file-option>] <old-name> <new-name>"),
	N_("git config remove-section [<file-option>] <name>"),
	N_("git config edit [<file-option>]"),
	N_("git config [<file-option>] --get-colorbool <name> [<stdout-is-tty>]"),
	NULL
};

static const char *const builtin_config_list_usage[] = {
	N_("git config list [<file-option>] [<display-option>] [--includes]"),
	NULL
};

static const char *const builtin_config_get_usage[] = {
	N_("git config get [<file-option>] [<display-option>] [--includes] [--all] [--regexp=<regexp>] [--value=<value>] [--fixed-value] [--default=<default>] <name>"),
	NULL
};

static const char *const builtin_config_set_usage[] = {
	N_("git config set [<file-option>] [--type=<type>] [--comment=<message>] [--all] [--value=<value>] [--fixed-value] <name> <value>"),
	NULL
};

static const char *const builtin_config_unset_usage[] = {
	N_("git config unset [<file-option>] [--all] [--value=<value>] [--fixed-value] <name> <value>"),
	NULL
};

static const char *const builtin_config_rename_section_usage[] = {
	N_("git config rename-section [<file-option>] <old-name> <new-name>"),
	NULL
};

static const char *const builtin_config_remove_section_usage[] = {
	N_("git config remove-section [<file-option>] <name>"),
	NULL
};

static const char *const builtin_config_edit_usage[] = {
	N_("git config edit [<file-option>]"),
	NULL
};

static char *key;
static regex_t *key_regexp;
static const char *value_pattern;
static regex_t *regexp;
static int show_keys;
static int omit_values;
static int use_key_regexp;
static int do_all;
static int do_not_match;
static char delim = '=';
static char key_delim = ' ';
static char term = '\n';

static parse_opt_subcommand_fn *subcommand;
static int use_global_config, use_system_config, use_local_config;
static int use_worktree_config;
static struct git_config_source given_config_source;
static int actions, type;
static char *default_value;
static int end_nul;
static int respect_includes_opt = -1;
static struct config_options config_options;
static int show_origin;
static int show_scope;
static int fixed_value;
static const char *comment_arg;

#define ACTION_GET (1<<0)
#define ACTION_GET_ALL (1<<1)
#define ACTION_GET_REGEXP (1<<2)
#define ACTION_REPLACE_ALL (1<<3)
#define ACTION_ADD (1<<4)
#define ACTION_UNSET (1<<5)
#define ACTION_UNSET_ALL (1<<6)
#define ACTION_RENAME_SECTION (1<<7)
#define ACTION_REMOVE_SECTION (1<<8)
#define ACTION_LIST (1<<9)
#define ACTION_EDIT (1<<10)
#define ACTION_SET (1<<11)
#define ACTION_SET_ALL (1<<12)
#define ACTION_GET_COLOR (1<<13)
#define ACTION_GET_COLORBOOL (1<<14)
#define ACTION_GET_URLMATCH (1<<15)

/*
 * The actions "ACTION_LIST | ACTION_GET_*" which may produce more than
 * one line of output and which should therefore be paged.
 */
#define PAGING_ACTIONS (ACTION_LIST | ACTION_GET_ALL | \
			ACTION_GET_REGEXP | ACTION_GET_URLMATCH)

#define TYPE_BOOL		1
#define TYPE_INT		2
#define TYPE_BOOL_OR_INT	3
#define TYPE_PATH		4
#define TYPE_EXPIRY_DATE	5
#define TYPE_COLOR		6
#define TYPE_BOOL_OR_STR	7

#define OPT_CALLBACK_VALUE(s, l, v, h, i) \
	{ OPTION_CALLBACK, (s), (l), (v), NULL, (h), PARSE_OPT_NOARG | \
	PARSE_OPT_NONEG, option_parse_type, (i) }

static NORETURN void usage_builtin_config(void);

static int option_parse_type(const struct option *opt, const char *arg,
			     int unset)
{
	int new_type, *to_type;

	if (unset) {
		*((int *) opt->value) = 0;
		return 0;
	}

	/*
	 * To support '--<type>' style flags, begin with new_type equal to
	 * opt->defval.
	 */
	new_type = opt->defval;
	if (!new_type) {
		if (!strcmp(arg, "bool"))
			new_type = TYPE_BOOL;
		else if (!strcmp(arg, "int"))
			new_type = TYPE_INT;
		else if (!strcmp(arg, "bool-or-int"))
			new_type = TYPE_BOOL_OR_INT;
		else if (!strcmp(arg, "bool-or-str"))
			new_type = TYPE_BOOL_OR_STR;
		else if (!strcmp(arg, "path"))
			new_type = TYPE_PATH;
		else if (!strcmp(arg, "expiry-date"))
			new_type = TYPE_EXPIRY_DATE;
		else if (!strcmp(arg, "color"))
			new_type = TYPE_COLOR;
		else
			die(_("unrecognized --type argument, %s"), arg);
	}

	to_type = opt->value;
	if (*to_type && *to_type != new_type) {
		/*
		 * Complain when there is a new type not equal to the old type.
		 * This allows for combinations like '--int --type=int' and
		 * '--type=int --type=int', but disallows ones like '--type=bool
		 * --int' and '--type=bool
		 * --type=int'.
		 */
		error(_("only one type at a time"));
		usage_builtin_config();
	}
	*to_type = new_type;

	return 0;
}

static void check_argc(int argc, int min, int max)
{
	if (argc >= min && argc <= max)
		return;
	if (min == max)
		error(_("wrong number of arguments, should be %d"), min);
	else
		error(_("wrong number of arguments, should be from %d to %d"),
		      min, max);
	usage_builtin_config();
}

static void show_config_origin(const struct key_value_info *kvi,
			       struct strbuf *buf)
{
	const char term = end_nul ? '\0' : '\t';

	strbuf_addstr(buf, config_origin_type_name(kvi->origin_type));
	strbuf_addch(buf, ':');
	if (end_nul)
		strbuf_addstr(buf, kvi->filename ? kvi->filename : "");
	else
		quote_c_style(kvi->filename ? kvi->filename : "", buf, NULL, 0);
	strbuf_addch(buf, term);
}

static void show_config_scope(const struct key_value_info *kvi,
			      struct strbuf *buf)
{
	const char term = end_nul ? '\0' : '\t';
	const char *scope = config_scope_name(kvi->scope);

	strbuf_addstr(buf, N_(scope));
	strbuf_addch(buf, term);
}

static int show_all_config(const char *key_, const char *value_,
			   const struct config_context *ctx,
			   void *cb UNUSED)
{
	const struct key_value_info *kvi = ctx->kvi;

	if (show_origin || show_scope) {
		struct strbuf buf = STRBUF_INIT;
		if (show_scope)
			show_config_scope(kvi, &buf);
		if (show_origin)
			show_config_origin(kvi, &buf);
		/* Use fwrite as "buf" can contain \0's if "end_null" is set. */
		fwrite(buf.buf, 1, buf.len, stdout);
		strbuf_release(&buf);
	}
	if (!omit_values && value_)
		printf("%s%c%s%c", key_, delim, value_, term);
	else
		printf("%s%c", key_, term);
	return 0;
}

struct strbuf_list {
	struct strbuf *items;
	int nr;
	int alloc;
};

static int format_config(struct strbuf *buf, const char *key_,
			 const char *value_, const struct key_value_info *kvi)
{
	if (show_scope)
		show_config_scope(kvi, buf);
	if (show_origin)
		show_config_origin(kvi, buf);
	if (show_keys)
		strbuf_addstr(buf, key_);
	if (!omit_values) {
		if (show_keys)
			strbuf_addch(buf, key_delim);

		if (type == TYPE_INT)
			strbuf_addf(buf, "%"PRId64,
				    git_config_int64(key_, value_ ? value_ : "", kvi));
		else if (type == TYPE_BOOL)
			strbuf_addstr(buf, git_config_bool(key_, value_) ?
				      "true" : "false");
		else if (type == TYPE_BOOL_OR_INT) {
			int is_bool, v;
			v = git_config_bool_or_int(key_, value_, kvi,
						   &is_bool);
			if (is_bool)
				strbuf_addstr(buf, v ? "true" : "false");
			else
				strbuf_addf(buf, "%d", v);
		} else if (type == TYPE_BOOL_OR_STR) {
			int v = git_parse_maybe_bool(value_);
			if (v < 0)
				strbuf_addstr(buf, value_);
			else
				strbuf_addstr(buf, v ? "true" : "false");
		} else if (type == TYPE_PATH) {
			const char *v;
			if (git_config_pathname(&v, key_, value_) < 0)
				return -1;
			strbuf_addstr(buf, v);
			free((char *)v);
		} else if (type == TYPE_EXPIRY_DATE) {
			timestamp_t t;
			if (git_config_expiry_date(&t, key_, value_) < 0)
				return -1;
			strbuf_addf(buf, "%"PRItime, t);
		} else if (type == TYPE_COLOR) {
			char v[COLOR_MAXLEN];
			if (git_config_color(v, key_, value_) < 0)
				return -1;
			strbuf_addstr(buf, v);
		} else if (value_) {
			strbuf_addstr(buf, value_);
		} else {
			/* Just show the key name; back out delimiter */
			if (show_keys)
				strbuf_setlen(buf, buf->len - 1);
		}
	}
	strbuf_addch(buf, term);
	return 0;
}

static int collect_config(const char *key_, const char *value_,
			  const struct config_context *ctx, void *cb)
{
	struct strbuf_list *values = cb;
	const struct key_value_info *kvi = ctx->kvi;

	if (!use_key_regexp && strcmp(key_, key))
		return 0;
	if (use_key_regexp && regexec(key_regexp, key_, 0, NULL, 0))
		return 0;
	if (fixed_value && strcmp(value_pattern, (value_?value_:"")))
		return 0;
	if (regexp != NULL &&
	    (do_not_match ^ !!regexec(regexp, (value_?value_:""), 0, NULL, 0)))
		return 0;

	ALLOC_GROW(values->items, values->nr + 1, values->alloc);
	strbuf_init(&values->items[values->nr], 0);

	return format_config(&values->items[values->nr++], key_, value_, kvi);
}

static int get_value(const char *key_, const char *regex_, unsigned flags)
{
	int ret = CONFIG_GENERIC_ERROR;
	struct strbuf_list values = {NULL};
	int i;

	if (use_key_regexp) {
		char *tl;

		/*
		 * NEEDSWORK: this naive pattern lowercasing obviously does not
		 * work for more complex patterns like "^[^.]*Foo.*bar".
		 * Perhaps we should deprecate this altogether someday.
		 */

		key = xstrdup(key_);
		for (tl = key + strlen(key) - 1;
		     tl >= key && *tl != '.';
		     tl--)
			*tl = tolower(*tl);
		for (tl = key; *tl && *tl != '.'; tl++)
			*tl = tolower(*tl);

		key_regexp = (regex_t*)xmalloc(sizeof(regex_t));
		if (regcomp(key_regexp, key, REG_EXTENDED)) {
			error(_("invalid key pattern: %s"), key_);
			FREE_AND_NULL(key_regexp);
			ret = CONFIG_INVALID_PATTERN;
			goto free_strings;
		}
	} else {
		if (git_config_parse_key(key_, &key, NULL)) {
			ret = CONFIG_INVALID_KEY;
			goto free_strings;
		}
	}

	if (regex_ && (flags & CONFIG_FLAGS_FIXED_VALUE))
		value_pattern = regex_;
	else if (regex_) {
		if (regex_[0] == '!') {
			do_not_match = 1;
			regex_++;
		}

		regexp = (regex_t*)xmalloc(sizeof(regex_t));
		if (regcomp(regexp, regex_, REG_EXTENDED)) {
			error(_("invalid pattern: %s"), regex_);
			FREE_AND_NULL(regexp);
			ret = CONFIG_INVALID_PATTERN;
			goto free_strings;
		}
	}

	config_with_options(collect_config, &values,
			    &given_config_source, the_repository,
			    &config_options);

	if (!values.nr && default_value) {
		struct key_value_info kvi = KVI_INIT;
		struct strbuf *item;

		kvi_from_param(&kvi);
		ALLOC_GROW(values.items, values.nr + 1, values.alloc);
		item = &values.items[values.nr++];
		strbuf_init(item, 0);
		if (format_config(item, key_, default_value, &kvi) < 0)
			die(_("failed to format default config value: %s"),
				default_value);
	}

	ret = !values.nr;

	for (i = 0; i < values.nr; i++) {
		struct strbuf *buf = values.items + i;
		if (do_all || i == values.nr - 1)
			fwrite(buf->buf, 1, buf->len, stdout);
		strbuf_release(buf);
	}
	free(values.items);

free_strings:
	free(key);
	if (key_regexp) {
		regfree(key_regexp);
		free(key_regexp);
	}
	if (regexp) {
		regfree(regexp);
		free(regexp);
	}

	return ret;
}

static char *normalize_value(const char *key, const char *value,
			     struct key_value_info *kvi)
{
	if (!value)
		return NULL;

	if (type == 0 || type == TYPE_PATH || type == TYPE_EXPIRY_DATE)
		/*
		 * We don't do normalization for TYPE_PATH here: If
		 * the path is like ~/foobar/, we prefer to store
		 * "~/foobar/" in the config file, and to expand the ~
		 * when retrieving the value.
		 * Also don't do normalization for expiry dates.
		 */
		return xstrdup(value);
	if (type == TYPE_INT)
		return xstrfmt("%"PRId64, git_config_int64(key, value, kvi));
	if (type == TYPE_BOOL)
		return xstrdup(git_config_bool(key, value) ?  "true" : "false");
	if (type == TYPE_BOOL_OR_INT) {
		int is_bool, v;
		v = git_config_bool_or_int(key, value, kvi, &is_bool);
		if (!is_bool)
			return xstrfmt("%d", v);
		else
			return xstrdup(v ? "true" : "false");
	}
	if (type == TYPE_BOOL_OR_STR) {
		int v = git_parse_maybe_bool(value);
		if (v < 0)
			return xstrdup(value);
		else
			return xstrdup(v ? "true" : "false");
	}
	if (type == TYPE_COLOR) {
		char v[COLOR_MAXLEN];
		if (git_config_color(v, key, value))
			die(_("cannot parse color '%s'"), value);

		/*
		 * The contents of `v` now contain an ANSI escape
		 * sequence, not suitable for including within a
		 * configuration file. Treat the above as a
		 * "sanity-check", and return the given value, which we
		 * know is representable as valid color code.
		 */
		return xstrdup(value);
	}

	BUG("cannot normalize type %d", type);
}

static int get_color_found;
static const char *get_color_slot;
static const char *get_colorbool_slot;
static char parsed_color[COLOR_MAXLEN];

static int git_get_color_config(const char *var, const char *value,
				const struct config_context *ctx UNUSED,
				void *cb UNUSED)
{
	if (!strcmp(var, get_color_slot)) {
		if (!value)
			config_error_nonbool(var);
		if (color_parse(value, parsed_color) < 0)
			return -1;
		get_color_found = 1;
	}
	return 0;
}

static void get_color(const char *var, const char *def_color)
{
	get_color_slot = var;
	get_color_found = 0;
	parsed_color[0] = '\0';
	config_with_options(git_get_color_config, NULL,
			    &given_config_source, the_repository,
			    &config_options);

	if (!get_color_found && def_color) {
		if (color_parse(def_color, parsed_color) < 0)
			die(_("unable to parse default color value"));
	}

	fputs(parsed_color, stdout);
}

static int get_colorbool_found;
static int get_diff_color_found;
static int get_color_ui_found;
static int git_get_colorbool_config(const char *var, const char *value,
				    const struct config_context *ctx UNUSED,
				    void *data UNUSED)
{
	if (!strcmp(var, get_colorbool_slot))
		get_colorbool_found = git_config_colorbool(var, value);
	else if (!strcmp(var, "diff.color"))
		get_diff_color_found = git_config_colorbool(var, value);
	else if (!strcmp(var, "color.ui"))
		get_color_ui_found = git_config_colorbool(var, value);
	return 0;
}

static int get_colorbool(const char *var, int print)
{
	get_colorbool_slot = var;
	get_colorbool_found = -1;
	get_diff_color_found = -1;
	get_color_ui_found = -1;
	config_with_options(git_get_colorbool_config, NULL,
			    &given_config_source, the_repository,
			    &config_options);

	if (get_colorbool_found < 0) {
		if (!strcmp(get_colorbool_slot, "color.diff"))
			get_colorbool_found = get_diff_color_found;
		if (get_colorbool_found < 0)
			get_colorbool_found = get_color_ui_found;
	}

	if (get_colorbool_found < 0)
		/* default value if none found in config */
		get_colorbool_found = GIT_COLOR_AUTO;

	get_colorbool_found = want_color(get_colorbool_found);

	if (print) {
		printf("%s\n", get_colorbool_found ? "true" : "false");
		return 0;
	} else
		return get_colorbool_found ? 0 : 1;
}

static void check_write(void)
{
	if (!given_config_source.file && !startup_info->have_repository)
		die(_("not in a git directory"));

	if (given_config_source.use_stdin)
		die(_("writing to stdin is not supported"));

	if (given_config_source.blob)
		die(_("writing config blobs is not supported"));
}

struct urlmatch_current_candidate_value {
	char value_is_null;
	struct strbuf value;
	struct key_value_info kvi;
};

static int urlmatch_collect_fn(const char *var, const char *value,
			       const struct config_context *ctx,
			       void *cb)
{
	struct string_list *values = cb;
	struct string_list_item *item = string_list_insert(values, var);
	struct urlmatch_current_candidate_value *matched = item->util;
	const struct key_value_info *kvi = ctx->kvi;

	if (!matched) {
		matched = xmalloc(sizeof(*matched));
		strbuf_init(&matched->value, 0);
		item->util = matched;
	} else {
		strbuf_reset(&matched->value);
	}
	matched->kvi = *kvi;

	if (value) {
		strbuf_addstr(&matched->value, value);
		matched->value_is_null = 0;
	} else {
		matched->value_is_null = 1;
	}
	return 0;
}

static int get_urlmatch(const char *var, const char *url)
{
	int ret;
	char *section_tail;
	struct string_list_item *item;
	struct urlmatch_config config = URLMATCH_CONFIG_INIT;
	struct string_list values = STRING_LIST_INIT_DUP;

	config.collect_fn = urlmatch_collect_fn;
	config.cascade_fn = NULL;
	config.cb = &values;

	if (!url_normalize(url, &config.url))
		die("%s", config.url.err);

	config.section = xstrdup_tolower(var);
	section_tail = strchr(config.section, '.');
	if (section_tail) {
		*section_tail = '\0';
		config.key = section_tail + 1;
		show_keys = 0;
	} else {
		config.key = NULL;
		show_keys = 1;
	}

	config_with_options(urlmatch_config_entry, &config,
			    &given_config_source, the_repository,
			    &config_options);

	ret = !values.nr;

	for_each_string_list_item(item, &values) {
		struct urlmatch_current_candidate_value *matched = item->util;
		struct strbuf buf = STRBUF_INIT;

		format_config(&buf, item->string,
			      matched->value_is_null ? NULL : matched->value.buf,
			      &matched->kvi);
		fwrite(buf.buf, 1, buf.len, stdout);
		strbuf_release(&buf);

		strbuf_release(&matched->value);
	}
	urlmatch_config_release(&config);
	string_list_clear(&values, 1);
	free(config.url.url);

	free((void *)config.section);
	return ret;
}

static char *default_user_config(void)
{
	struct strbuf buf = STRBUF_INIT;
	strbuf_addf(&buf,
		    _("# This is Git's per-user configuration file.\n"
		      "[user]\n"
		      "# Please adapt and uncomment the following lines:\n"
		      "#	name = %s\n"
		      "#	email = %s\n"),
		    ident_default_name(),
		    ident_default_email());
	return strbuf_detach(&buf, NULL);
}

static void handle_config_location(const char *prefix)
{
	if (use_global_config + use_system_config + use_local_config +
	    use_worktree_config +
	    !!given_config_source.file + !!given_config_source.blob > 1) {
		error(_("only one config file at a time"));
		usage_builtin_config();
	}

	if (!startup_info->have_repository) {
		if (use_local_config)
			die(_("--local can only be used inside a git repository"));
		if (given_config_source.blob)
			die(_("--blob can only be used inside a git repository"));
		if (use_worktree_config)
			die(_("--worktree can only be used inside a git repository"));
	}

	if (given_config_source.file &&
			!strcmp(given_config_source.file, "-")) {
		given_config_source.file = NULL;
		given_config_source.use_stdin = 1;
		given_config_source.scope = CONFIG_SCOPE_COMMAND;
	}

	if (use_global_config) {
		given_config_source.file = git_global_config();
		if (!given_config_source.file)
			/*
			 * It is unknown if HOME/.gitconfig exists, so
			 * we do not know if we should write to XDG
			 * location; error out even if XDG_CONFIG_HOME
			 * is set and points at a sane location.
			 */
			die(_("$HOME not set"));
		given_config_source.scope = CONFIG_SCOPE_GLOBAL;
	} else if (use_system_config) {
		given_config_source.file = git_system_config();
		given_config_source.scope = CONFIG_SCOPE_SYSTEM;
	} else if (use_local_config) {
		given_config_source.file = git_pathdup("config");
		given_config_source.scope = CONFIG_SCOPE_LOCAL;
	} else if (use_worktree_config) {
		struct worktree **worktrees = get_worktrees();
		if (the_repository->repository_format_worktree_config)
			given_config_source.file = git_pathdup("config.worktree");
		else if (worktrees[0] && worktrees[1])
			die(_("--worktree cannot be used with multiple "
			      "working trees unless the config\n"
			      "extension worktreeConfig is enabled. "
			      "Please read \"CONFIGURATION FILE\"\n"
			      "section in \"git help worktree\" for details"));
		else
			given_config_source.file = git_pathdup("config");
		given_config_source.scope = CONFIG_SCOPE_LOCAL;
		free_worktrees(worktrees);
	} else if (given_config_source.file) {
		if (!is_absolute_path(given_config_source.file) && prefix)
			given_config_source.file =
				prefix_filename(prefix, given_config_source.file);
		given_config_source.scope = CONFIG_SCOPE_COMMAND;
	} else if (given_config_source.blob) {
		given_config_source.scope = CONFIG_SCOPE_COMMAND;
	}

	if (respect_includes_opt == -1)
		config_options.respect_includes = !given_config_source.file;
	else
		config_options.respect_includes = respect_includes_opt;
	if (startup_info->have_repository) {
		config_options.commondir = get_git_common_dir();
		config_options.git_dir = get_git_dir();
	}
}

static void handle_nul(void) {
	if (end_nul) {
		term = '\0';
		delim = '\n';
		key_delim = '\n';
	}
}

#define CONFIG_LOCATION_OPTIONS \
	OPT_GROUP(N_("Config file location")), \
	OPT_BOOL(0, "global", &use_global_config, N_("use global config file")), \
	OPT_BOOL(0, "system", &use_system_config, N_("use system config file")), \
	OPT_BOOL(0, "local", &use_local_config, N_("use repository config file")), \
	OPT_BOOL(0, "worktree", &use_worktree_config, N_("use per-worktree config file")), \
	OPT_STRING('f', "file", &given_config_source.file, N_("file"), N_("use given config file")), \
	OPT_STRING(0, "blob", &given_config_source.blob, N_("blob-id"), N_("read config from given blob object"))

#define CONFIG_TYPE_OPTIONS \
	OPT_GROUP(N_("Type")), \
	OPT_CALLBACK('t', "type", &type, N_("type"), N_("value is given this type"), option_parse_type), \
	OPT_CALLBACK_VALUE(0, "bool", &type, N_("value is \"true\" or \"false\""), TYPE_BOOL), \
	OPT_CALLBACK_VALUE(0, "int", &type, N_("value is decimal number"), TYPE_INT), \
	OPT_CALLBACK_VALUE(0, "bool-or-int", &type, N_("value is --bool or --int"), TYPE_BOOL_OR_INT), \
	OPT_CALLBACK_VALUE(0, "bool-or-str", &type, N_("value is --bool or string"), TYPE_BOOL_OR_STR), \
	OPT_CALLBACK_VALUE(0, "path", &type, N_("value is a path (file or directory name)"), TYPE_PATH), \
	OPT_CALLBACK_VALUE(0, "expiry-date", &type, N_("value is an expiry date"), TYPE_EXPIRY_DATE)

#define CONFIG_DISPLAY_OPTIONS \
	OPT_GROUP(N_("Display options")), \
	OPT_BOOL('z', "null", &end_nul, N_("terminate values with NUL byte")), \
	OPT_BOOL(0, "name-only", &omit_values, N_("show variable names only")), \
	OPT_BOOL(0, "show-origin", &show_origin, N_("show origin of config (file, standard input, blob, command line)")), \
	OPT_BOOL(0, "show-scope", &show_scope, N_("show scope of config (worktree, local, global, system, command)"))

static struct option builtin_config_options[] = {
	CONFIG_LOCATION_OPTIONS,
	OPT_GROUP(N_("Action")),
	OPT_CMDMODE(0, "get", &actions, N_("get value: name [<value-pattern>]"), ACTION_GET),
	OPT_CMDMODE(0, "get-all", &actions, N_("get all values: key [<value-pattern>]"), ACTION_GET_ALL),
	OPT_CMDMODE(0, "get-regexp", &actions, N_("get values for regexp: name-regex [<value-pattern>]"), ACTION_GET_REGEXP),
	OPT_CMDMODE(0, "get-urlmatch", &actions, N_("get value specific for the URL: section[.var] URL"), ACTION_GET_URLMATCH),
	OPT_CMDMODE(0, "replace-all", &actions, N_("replace all matching variables: name value [<value-pattern>]"), ACTION_REPLACE_ALL),
	OPT_CMDMODE(0, "add", &actions, N_("add a new variable: name value"), ACTION_ADD),
	OPT_CMDMODE(0, "unset", &actions, N_("remove a variable: name [<value-pattern>]"), ACTION_UNSET),
	OPT_CMDMODE(0, "unset-all", &actions, N_("remove all matches: name [<value-pattern>]"), ACTION_UNSET_ALL),
	OPT_CMDMODE(0, "rename-section", &actions, N_("rename section: old-name new-name"), ACTION_RENAME_SECTION),
	OPT_CMDMODE(0, "remove-section", &actions, N_("remove a section: name"), ACTION_REMOVE_SECTION),
	OPT_CMDMODE('l', "list", &actions, N_("list all"), ACTION_LIST),
	OPT_CMDMODE('e', "edit", &actions, N_("open an editor"), ACTION_EDIT),
	OPT_CMDMODE(0, "get-color", &actions, N_("find the color configured: slot [<default>]"), ACTION_GET_COLOR),
	OPT_CMDMODE(0, "get-colorbool", &actions, N_("find the color setting: slot [<stdout-is-tty>]"), ACTION_GET_COLORBOOL),
	CONFIG_TYPE_OPTIONS,
	CONFIG_DISPLAY_OPTIONS,
	OPT_GROUP(N_("Other")),
	OPT_STRING(0, "default", &default_value, N_("value"), N_("with --get, use default value when missing entry")),
	OPT_STRING(0, "comment", &comment_arg, N_("value"), N_("human-readable comment string (# will be prepended as needed)")),
	OPT_BOOL(0, "fixed-value", &fixed_value, N_("use string equality when comparing values to 'value-pattern'")),
	OPT_BOOL(0, "includes", &respect_includes_opt, N_("respect include directives on lookup")),
	OPT_END(),
};

static NORETURN void usage_builtin_config(void)
{
	usage_with_options(builtin_config_usage, builtin_config_options);
}

static int cmd_config_list(int argc, const char **argv, const char *prefix)
{
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		CONFIG_DISPLAY_OPTIONS,
		OPT_GROUP(N_("Other")),
		OPT_BOOL(0, "includes", &respect_includes_opt, N_("respect include directives on lookup")),
		OPT_END(),
	};

	argc = parse_options(argc, argv, prefix, opts, builtin_config_list_usage, 0);
	check_argc(argc, 0, 0);

	handle_config_location(prefix);
	handle_nul();

	setup_auto_pager("config", 1);

	if (config_with_options(show_all_config, NULL,
				&given_config_source, the_repository,
				&config_options) < 0) {
		if (given_config_source.file)
			die_errno(_("unable to read config file '%s'"),
				  given_config_source.file);
		else
			die(_("error processing config file(s)"));
	}

	return 0;
}

static int cmd_config_get(int argc, const char **argv, const char *prefix)
{
	const char *value_pattern = NULL, *url = NULL;
	int flags = 0;
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		CONFIG_TYPE_OPTIONS,
		OPT_GROUP(N_("Filter options")),
		OPT_BOOL(0, "all", &do_all, N_("return all values for multi-valued config options")),
		OPT_BOOL(0, "regexp", &use_key_regexp, N_("interpret the name as a regular expression")),
		OPT_STRING(0, "value", &value_pattern, N_("pattern"), N_("show config with values matching the pattern")),
		OPT_BIT(0, "fixed-value", &flags, N_("use string equality when comparing values to value pattern"), CONFIG_FLAGS_FIXED_VALUE),
		OPT_STRING(0, "url", &url, N_("URL"), N_("show config matching the given URL")),
		CONFIG_DISPLAY_OPTIONS,
		OPT_BOOL(0, "show-names", &show_keys, N_("show config keys in addition to their values")),
		OPT_GROUP(N_("Other")),
		OPT_BOOL(0, "includes", &respect_includes_opt, N_("respect include directives on lookup")),
		OPT_STRING(0, "default", &default_value, N_("value"), N_("use default value when missing entry")),
		OPT_END(),
	};

	argc = parse_options(argc, argv, prefix, opts, builtin_config_get_usage,
			     PARSE_OPT_STOP_AT_NON_OPTION);
	check_argc(argc, 1, 1);

	if ((flags & CONFIG_FLAGS_FIXED_VALUE) && !value_pattern)
		die(_("--fixed-value only applies with 'value-pattern'"));
	if (default_value && (do_all || url))
		die(_("--default= cannot be used with --all or --url="));
	if (url && (do_all || use_key_regexp || value_pattern))
		die(_("--url= cannot be used with --all, --regexp or --value"));

	handle_config_location(prefix);
	handle_nul();

	setup_auto_pager("config", 1);

	if (url)
		return get_urlmatch(argv[0], url);
	return get_value(argv[0], value_pattern, flags);
}

static int cmd_config_set(int argc, const char **argv, const char *prefix)
{
	const char *value_pattern = NULL, *comment_arg = NULL;
	char *comment = NULL;
	int flags = 0, append = 0;
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		CONFIG_TYPE_OPTIONS,
		OPT_GROUP(N_("Filter")),
		OPT_BIT(0, "all", &flags, N_("replace multi-valued config option with new value"), CONFIG_FLAGS_MULTI_REPLACE),
		OPT_STRING(0, "value", &value_pattern, N_("pattern"), N_("show config with values matching the pattern")),
		OPT_BIT(0, "fixed-value", &flags, N_("use string equality when comparing values to value pattern"), CONFIG_FLAGS_FIXED_VALUE),
		OPT_GROUP(N_("Other")),
		OPT_STRING(0, "comment", &comment_arg, N_("value"), N_("human-readable comment string (# will be prepended as needed)")),
		OPT_BOOL(0, "append", &append, N_("add a new line without altering any existing values")),
		OPT_END(),
	};
	struct key_value_info default_kvi = KVI_INIT;
	char *value;
	int ret;

	argc = parse_options(argc, argv, prefix, opts, builtin_config_set_usage,
			     PARSE_OPT_STOP_AT_NON_OPTION);
	check_write();
	check_argc(argc, 2, 2);

	if ((flags & CONFIG_FLAGS_FIXED_VALUE) && !value_pattern)
		die(_("--fixed-value only applies with --value=<pattern>"));
	if (append && value_pattern)
		die(_("--append cannot be used with --value=<pattern>"));
	if (append)
		value_pattern = CONFIG_REGEX_NONE;

	comment = git_config_prepare_comment_string(comment_arg);

	handle_config_location(prefix);

	value = normalize_value(argv[0], argv[1], &default_kvi);

	if ((flags & CONFIG_FLAGS_MULTI_REPLACE) || value_pattern) {
		ret = git_config_set_multivar_in_file_gently(given_config_source.file,
							     argv[0], value, value_pattern,
							     comment, flags);
	} else {
		ret = git_config_set_in_file_gently(given_config_source.file,
						    argv[0], comment, value);
		if (ret == CONFIG_NOTHING_SET)
			error(_("cannot overwrite multiple values with a single value\n"
			"       Use a regexp, --add or --replace-all to change %s."), argv[0]);
	}

	free(comment);
	free(value);
	return ret;
}

static int cmd_config_unset(int argc, const char **argv, const char *prefix)
{
	const char *value_pattern = NULL;
	int flags = 0;
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		OPT_GROUP(N_("Filter")),
		OPT_BIT(0, "all", &flags, N_("replace multi-valued config option with new value"), CONFIG_FLAGS_MULTI_REPLACE),
		OPT_STRING(0, "value", &value_pattern, N_("pattern"), N_("show config with values matching the pattern")),
		OPT_BIT(0, "fixed-value", &flags, N_("use string equality when comparing values to value pattern"), CONFIG_FLAGS_FIXED_VALUE),
		OPT_END(),
	};

	argc = parse_options(argc, argv, prefix, opts, builtin_config_unset_usage,
			     PARSE_OPT_STOP_AT_NON_OPTION);
	check_write();
	check_argc(argc, 1, 1);

	if ((flags & CONFIG_FLAGS_FIXED_VALUE) && !value_pattern)
		die(_("--fixed-value only applies with 'value-pattern'"));

	handle_config_location(prefix);

	if ((flags & CONFIG_FLAGS_MULTI_REPLACE) || value_pattern)
		return git_config_set_multivar_in_file_gently(given_config_source.file,
							      argv[0], NULL, value_pattern,
							      NULL, flags);
	else
		return git_config_set_in_file_gently(given_config_source.file, argv[0],
						     NULL, NULL);
}

static int cmd_config_rename_section(int argc, const char **argv, const char *prefix)
{
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		OPT_END(),
	};
	int ret;

	argc = parse_options(argc, argv, prefix, opts, builtin_config_rename_section_usage,
			     PARSE_OPT_STOP_AT_NON_OPTION);
	check_write();
	check_argc(argc, 2, 2);

	handle_config_location(prefix);

	ret = git_config_rename_section_in_file(given_config_source.file,
						argv[0], argv[1]);
	if (ret < 0)
		return ret;
	else if (!ret)
		die(_("no such section: %s"), argv[0]);

	return 0;
}

static int cmd_config_remove_section(int argc, const char **argv, const char *prefix)
{
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		OPT_END(),
	};
	int ret;

	argc = parse_options(argc, argv, prefix, opts, builtin_config_remove_section_usage,
			     PARSE_OPT_STOP_AT_NON_OPTION);
	check_write();
	check_argc(argc, 1, 1);

	handle_config_location(prefix);

	ret = git_config_rename_section_in_file(given_config_source.file,
						argv[0], NULL);
	if (ret < 0)
		return ret;
	else if (!ret)
		die(_("no such section: %s"), argv[0]);

	return 0;
}

static int show_editor(void)
{
	char *config_file;

	if (!given_config_source.file && !startup_info->have_repository)
		die(_("not in a git directory"));
	if (given_config_source.use_stdin)
		die(_("editing stdin is not supported"));
	if (given_config_source.blob)
		die(_("editing blobs is not supported"));
	git_config(git_default_config, NULL);
	config_file = given_config_source.file ?
			xstrdup(given_config_source.file) :
			git_pathdup("config");
	if (use_global_config) {
		int fd = open(config_file, O_CREAT | O_EXCL | O_WRONLY, 0666);
		if (fd >= 0) {
			char *content = default_user_config();
			write_str_in_full(fd, content);
			free(content);
			close(fd);
		}
		else if (errno != EEXIST)
			die_errno(_("cannot create configuration file %s"), config_file);
	}
	launch_editor(config_file, NULL, NULL);
	free(config_file);

	return 0;
}

static int cmd_config_edit(int argc, const char **argv, const char *prefix)
{
	struct option opts[] = {
		CONFIG_LOCATION_OPTIONS,
		OPT_END(),
	};

	argc = parse_options(argc, argv, prefix, opts, builtin_config_edit_usage, 0);
	check_write();
	check_argc(argc, 0, 0);

	handle_config_location(prefix);

	return show_editor();
}

static struct option builtin_subcommand_options[] = {
	OPT_SUBCOMMAND("list", &subcommand, cmd_config_list),
	OPT_SUBCOMMAND("get", &subcommand, cmd_config_get),
	OPT_SUBCOMMAND("set", &subcommand, cmd_config_set),
	OPT_SUBCOMMAND("unset", &subcommand, cmd_config_unset),
	OPT_SUBCOMMAND("rename-section", &subcommand, cmd_config_rename_section),
	OPT_SUBCOMMAND("remove-section", &subcommand, cmd_config_remove_section),
	OPT_SUBCOMMAND("edit", &subcommand, cmd_config_edit),
	OPT_END(),
};

int cmd_config(int argc, const char **argv, const char *prefix)
{
	char *value = NULL, *comment = NULL;
	int flags = 0;
	int ret = 0;
	struct key_value_info default_kvi = KVI_INIT;

	given_config_source.file = xstrdup_or_null(getenv(CONFIG_ENVIRONMENT));

	/*
	 * This is somewhat hacky: we first parse the command line while
	 * keeping all args intact in order to determine whether a subcommand
	 * has been specified. If so, we re-parse it a second time, but this
	 * time we drop KEEP_ARGV0. This is so that we don't munge the command
	 * line in case no subcommand was given, which would otherwise confuse
	 * us when parsing the legacy-style modes that don't use subcommands.
	 */
	argc = parse_options(argc, argv, prefix, builtin_subcommand_options, builtin_config_usage,
			     PARSE_OPT_SUBCOMMAND_OPTIONAL|PARSE_OPT_KEEP_ARGV0|PARSE_OPT_KEEP_UNKNOWN_OPT);
	if (subcommand) {
		argc = parse_options(argc, argv, prefix, builtin_subcommand_options, builtin_config_usage,
		       PARSE_OPT_SUBCOMMAND_OPTIONAL|PARSE_OPT_KEEP_UNKNOWN_OPT);
		return subcommand(argc, argv, prefix);
	}

	argc = parse_options(argc, argv, prefix, builtin_config_options,
			     builtin_config_usage,
			     PARSE_OPT_STOP_AT_NON_OPTION);

	handle_config_location(prefix);
	handle_nul();

	if ((actions & (ACTION_GET_COLOR|ACTION_GET_COLORBOOL)) && type) {
		error(_("--get-color and variable type are incoherent"));
		usage_builtin_config();
	}

	if (actions == 0)
		switch (argc) {
		case 1: actions = ACTION_GET; break;
		case 2: actions = ACTION_SET; break;
		case 3: actions = ACTION_SET_ALL; break;
		default:
			usage_builtin_config();
		}
	if (omit_values &&
	    !(actions == ACTION_LIST || actions == ACTION_GET_REGEXP)) {
		error(_("--name-only is only applicable to --list or --get-regexp"));
		usage_builtin_config();
	}

	if (show_origin && !(actions &
		(ACTION_GET|ACTION_GET_ALL|ACTION_GET_REGEXP|ACTION_LIST))) {
		error(_("--show-origin is only applicable to --get, --get-all, "
			"--get-regexp, and --list"));
		usage_builtin_config();
	}

	if (default_value && !(actions & ACTION_GET)) {
		error(_("--default is only applicable to --get"));
		usage_builtin_config();
	}

	if (comment_arg &&
	    !(actions & (ACTION_ADD|ACTION_SET|ACTION_SET_ALL|ACTION_REPLACE_ALL))) {
		error(_("--comment is only applicable to add/set/replace operations"));
		usage_builtin_config();
	}

	/* check usage of --fixed-value */
	if (fixed_value) {
		int allowed_usage = 0;

		switch (actions) {
		/* git config --get <name> <value-pattern> */
		case ACTION_GET:
		/* git config --get-all <name> <value-pattern> */
		case ACTION_GET_ALL:
		/* git config --get-regexp <name-pattern> <value-pattern> */
		case ACTION_GET_REGEXP:
		/* git config --unset <name> <value-pattern> */
		case ACTION_UNSET:
		/* git config --unset-all <name> <value-pattern> */
		case ACTION_UNSET_ALL:
			allowed_usage = argc > 1 && !!argv[1];
			break;

		/* git config <name> <value> <value-pattern> */
		case ACTION_SET_ALL:
		/* git config --replace-all <name> <value> <value-pattern> */
		case ACTION_REPLACE_ALL:
			allowed_usage = argc > 2 && !!argv[2];
			break;

		/* other options don't allow --fixed-value */
		}

		if (!allowed_usage) {
			error(_("--fixed-value only applies with 'value-pattern'"));
			usage_builtin_config();
		}

		flags |= CONFIG_FLAGS_FIXED_VALUE;
	}

	comment = git_config_prepare_comment_string(comment_arg);

	if (actions & PAGING_ACTIONS)
		setup_auto_pager("config", 1);

	if (actions == ACTION_LIST) {
		check_argc(argc, 0, 0);
		if (config_with_options(show_all_config, NULL,
					&given_config_source, the_repository,
					&config_options) < 0) {
			if (given_config_source.file)
				die_errno(_("unable to read config file '%s'"),
					  given_config_source.file);
			else
				die(_("error processing config file(s)"));
		}
	}
	else if (actions == ACTION_EDIT) {
		ret = show_editor();
	}
	else if (actions == ACTION_SET) {
		check_write();
		check_argc(argc, 2, 2);
		value = normalize_value(argv[0], argv[1], &default_kvi);
		ret = git_config_set_in_file_gently(given_config_source.file, argv[0], comment, value);
		if (ret == CONFIG_NOTHING_SET)
			error(_("cannot overwrite multiple values with a single value\n"
			"       Use a regexp, --add or --replace-all to change %s."), argv[0]);
	}
	else if (actions == ACTION_SET_ALL) {
		check_write();
		check_argc(argc, 2, 3);
		value = normalize_value(argv[0], argv[1], &default_kvi);
		ret = git_config_set_multivar_in_file_gently(given_config_source.file,
							     argv[0], value, argv[2],
							     comment, flags);
	}
	else if (actions == ACTION_ADD) {
		check_write();
		check_argc(argc, 2, 2);
		value = normalize_value(argv[0], argv[1], &default_kvi);
		ret = git_config_set_multivar_in_file_gently(given_config_source.file,
							     argv[0], value,
							     CONFIG_REGEX_NONE,
							     comment, flags);
	}
	else if (actions == ACTION_REPLACE_ALL) {
		check_write();
		check_argc(argc, 2, 3);
		value = normalize_value(argv[0], argv[1], &default_kvi);
		ret = git_config_set_multivar_in_file_gently(given_config_source.file,
							     argv[0], value, argv[2],
							     comment, flags | CONFIG_FLAGS_MULTI_REPLACE);
	}
	else if (actions == ACTION_GET) {
		check_argc(argc, 1, 2);
		return get_value(argv[0], argv[1], flags);
	}
	else if (actions == ACTION_GET_ALL) {
		do_all = 1;
		check_argc(argc, 1, 2);
		return get_value(argv[0], argv[1], flags);
	}
	else if (actions == ACTION_GET_REGEXP) {
		show_keys = 1;
		use_key_regexp = 1;
		do_all = 1;
		check_argc(argc, 1, 2);
		return get_value(argv[0], argv[1], flags);
	}
	else if (actions == ACTION_GET_URLMATCH) {
		check_argc(argc, 2, 2);
		return get_urlmatch(argv[0], argv[1]);
	}
	else if (actions == ACTION_UNSET) {
		check_write();
		check_argc(argc, 1, 2);
		if (argc == 2)
			return git_config_set_multivar_in_file_gently(given_config_source.file,
								      argv[0], NULL, argv[1],
								      NULL, flags);
		else
			return git_config_set_in_file_gently(given_config_source.file,
							     argv[0], NULL, NULL);
	}
	else if (actions == ACTION_UNSET_ALL) {
		check_write();
		check_argc(argc, 1, 2);
		return git_config_set_multivar_in_file_gently(given_config_source.file,
							      argv[0], NULL, argv[1],
							      NULL, flags | CONFIG_FLAGS_MULTI_REPLACE);
	}
	else if (actions == ACTION_RENAME_SECTION) {
		check_write();
		check_argc(argc, 2, 2);
		ret = git_config_rename_section_in_file(given_config_source.file,
							argv[0], argv[1]);
		if (ret < 0)
			return ret;
		else if (!ret)
			die(_("no such section: %s"), argv[0]);
		else
			ret = 0;
	}
	else if (actions == ACTION_REMOVE_SECTION) {
		check_write();
		check_argc(argc, 1, 1);
		ret = git_config_rename_section_in_file(given_config_source.file,
							argv[0], NULL);
		if (ret < 0)
			return ret;
		else if (!ret)
			die(_("no such section: %s"), argv[0]);
		else
			ret = 0;
	}
	else if (actions == ACTION_GET_COLOR) {
		check_argc(argc, 1, 2);
		get_color(argv[0], argv[1]);
	}
	else if (actions == ACTION_GET_COLORBOOL) {
		check_argc(argc, 1, 2);
		if (argc == 2)
			color_stdout_is_tty = git_config_bool("command line", argv[1]);
		return get_colorbool(argv[0], argc == 2);
	}

	free(comment);
	free(value);
	return ret;
}
