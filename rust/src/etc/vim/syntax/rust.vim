" Vim syntax file
" Language:     Rust
" Maintainer:   Patrick Walton <pcwalton@mozilla.com>
" Maintainer:   Ben Blum <bblum@cs.cmu.edu>
" Maintainer:   Chris Morgan <me@chrismorgan.info>
" Last Change:  2013 Aug 1

if version < 600
  syntax clear
elseif exists("b:current_syntax")
  finish
endif

syn keyword   rustConditional match if else
syn keyword   rustOperator    as

syn match     rustAssert      "\<assert\(\w\)*!" contained
syn match     rustFail        "\<fail\(\w\)*!" contained
syn keyword   rustKeyword     break copy do extern
syn keyword   rustKeyword     in if impl let log
syn keyword   rustKeyword     copy do extern
syn keyword   rustKeyword     for impl let log
syn keyword   rustKeyword     loop mod once priv pub
syn keyword   rustKeyword     return
syn keyword   rustKeyword     unsafe while
syn keyword   rustKeyword     use nextgroup=rustModPath skipwhite
" FIXME: Scoped impl's name is also fallen in this category
syn keyword   rustKeyword     mod trait struct enum type nextgroup=rustIdentifier skipwhite
syn keyword   rustKeyword     fn nextgroup=rustFuncName skipwhite
syn keyword   rustStorage     const mut ref static

syn match     rustIdentifier  contains=rustIdentifierPrime "\%([^[:cntrl:][:space:][:punct:][:digit:]]\|_\)\%([^[:cntrl:][:punct:][:space:]]\|_\)*" display contained
syn match     rustFuncName    "\%([^[:cntrl:][:space:][:punct:][:digit:]]\|_\)\%([^[:cntrl:][:punct:][:space:]]\|_\)*" display contained

" reserved
syn keyword   rustKeyword     be

syn keyword   rustType        int uint float char bool u8 u16 u32 u64 f32
syn keyword   rustType        f64 i8 i16 i32 i64 str Self
syn keyword   rustType        Option Either

" Types from libc
syn keyword   rustType        c_float c_double c_void FILE fpos_t
syn keyword   rustType        DIR dirent
syn keyword   rustType        c_char c_schar c_uchar
syn keyword   rustType        c_short c_ushort c_int c_uint c_long c_ulong
syn keyword   rustType        size_t ptrdiff_t clock_t time_t
syn keyword   rustType        c_longlong c_ulonglong intptr_t uintptr_t
syn keyword   rustType        off_t dev_t ino_t pid_t mode_t ssize_t

syn keyword   rustTrait       Const Copy Send Owned Sized " inherent traits
syn keyword   rustTrait       Clone Decodable Encodable IterBytes Rand ToStr
syn keyword   rustTrait       Eq Ord TotalEq TotalOrd Num Ptr
syn keyword   rustTrait       Drop Add Sub Mul Quot Rem Neg BitAnd BitOr
syn keyword   rustTrait       BitXor Shl Shr Index

syn keyword   rustSelf        self
syn keyword   rustBoolean     true false

syn keyword   rustConstant    Some None       " option
syn keyword   rustConstant    Left Right      " either
syn keyword   rustConstant    Ok Err          " result
syn keyword   rustConstant    Success Failure " task
syn keyword   rustConstant    Cons Nil        " list
" syn keyword   rustConstant    empty node      " tree

" Constants from libc
syn keyword   rustConstant    EXIT_FAILURE EXIT_SUCCESS RAND_MAX
syn keyword   rustConstant    EOF SEEK_SET SEEK_CUR SEEK_END _IOFBF _IONBF
syn keyword   rustConstant    _IOLBF BUFSIZ FOPEN_MAX FILENAME_MAX L_tmpnam
syn keyword   rustConstant    TMP_MAX O_RDONLY O_WRONLY O_RDWR O_APPEND O_CREAT
syn keyword   rustConstant    O_EXCL O_TRUNC S_IFIFO S_IFCHR S_IFBLK S_IFDIR
syn keyword   rustConstant    S_IFREG S_IFMT S_IEXEC S_IWRITE S_IREAD S_IRWXU
syn keyword   rustConstant    S_IXUSR S_IWUSR S_IRUSR F_OK R_OK W_OK X_OK
syn keyword   rustConstant    STDIN_FILENO STDOUT_FILENO STDERR_FILENO

" If foo::bar changes to foo.bar, change this ("::" to "\.").
" If foo::bar changes to Foo::bar, change this (first "\w" to "\u").
syn match     rustModPath     "\w\(\w\)*::[^<]"he=e-3,me=e-3
syn match     rustModPath     "\w\(\w\)*" contained " only for 'use path;'
syn match     rustModPathSep  "::"

syn match     rustFuncCall    "\w\(\w\)*("he=e-1,me=e-1
syn match     rustFuncCall    "\w\(\w\)*::<"he=e-3,me=e-3 " foo::<T>();

" This is merely a convention; note also the use of [A-Z], restricting it to
" latin identifiers rather than the full Unicode uppercase. I have not used
" [:upper:] as it depends upon 'noignorecase'
"syn match     rustCapsIdent    display "[A-Z]\w\(\w\)*"

syn match     rustOperator     display "\%(+\|-\|/\|*\|=\|\^\|&\||\|!\|>\|<\|%\)=\?"
" This one isn't *quite* right, as we could have binary-& with a reference
syn match     rustSigil        display /&\s\+[&~@*][^)= \t\r\n]/he=e-1,me=e-1
syn match     rustSigil        display /[&~@*][^)= \t\r\n]/he=e-1,me=e-1
" This isn't actually correct; a closure with no arguments can be `|| { }`.
" Last, because the & in && isn't a sigil
syn match     rustOperator     display "&&\|||"

syn match     rustMacro       '\w\(\w\)*!' contains=rustAssert,rustFail
syn match     rustMacro       '#\w\(\w\)*' contains=rustAssert,rustFail

syn match     rustFormat      display "%\(\d\+\$\)\=[-+' #0*]*\(\d*\|\*\|\*\d\+\$\)\(\.\(\d*\|\*\|\*\d\+\$\)\)\=\([hlLjzt]\|ll\|hh\)\=\([aAbdiuoxXDOUfFeEgGcCsSpn?]\|\[\^\=.[^]]*\]\)" contained
syn match     rustFormat      display "%%" contained
syn match     rustSpecial     display contained /\\\([nrt\\'"]\|x\x\{2}\|u\x\{4}\|U\x\{8}\)/
syn region    rustString      start=+L\="+ skip=+\\\\\|\\"+ end=+"+ contains=rustTodo,rustFormat,rustSpecial

syn region    rustAttribute   start="#\[" end="\]" contains=rustString,rustDeriving
syn region    rustDeriving    start="deriving(" end=")" contained contains=rustTrait

" Number literals
syn match     rustNumber      display "\<[0-9][0-9_]*\>"
syn match     rustNumber      display "\<[0-9][0-9_]*\(u\|u8\|u16\|u32\|u64\)\>"
syn match     rustNumber      display "\<[0-9][0-9_]*\(i\|i8\|i16\|i32\|i64\)\>"

syn match     rustHexNumber   display "\<0x[a-fA-F0-9_]\+\>"
syn match     rustHexNumber   display "\<0x[a-fA-F0-9_]\+\(u\|u8\|u16\|u32\|u64\)\>"
syn match     rustHexNumber   display "\<0x[a-fA-F0-9_]\+\(i8\|i16\|i32\|i64\)\>"
syn match     rustBinNumber   display "\<0b[01_]\+\>"
syn match     rustBinNumber   display "\<0b[01_]\+\(u\|u8\|u16\|u32\|u64\)\>"
syn match     rustBinNumber   display "\<0b[01_]\+\(i8\|i16\|i32\|i64\)\>"

syn match     rustFloat       display "\<[0-9][0-9_]*\(f\|f32\|f64\)\>"
syn match     rustFloat       display "\<[0-9][0-9_]*\([eE][+-]\=[0-9_]\+\)\>"
syn match     rustFloat       display "\<[0-9][0-9_]*\([eE][+-]\=[0-9_]\+\)\(f\|f32\|f64\)\>"
syn match     rustFloat       display "\<[0-9][0-9_]*\.[0-9_]\+\>"
syn match     rustFloat       display "\<[0-9][0-9_]*\.[0-9_]\+\(f\|f32\|f64\)\>"
syn match     rustFloat       display "\<[0-9][0-9_]*\.[0-9_]\+\%([eE][+-]\=[0-9_]\+\)\>"
syn match     rustFloat       display "\<[0-9][0-9_]*\.[0-9_]\+\%([eE][+-]\=[0-9_]\+\)\(f\|f32\|f64\)\>"

" For the benefit of delimitMate
syn region rustLifetimeCandidate display start=/&'\%(\([^'\\]\|\\\(['nrt\\\"]\|x\x\{2}\|u\x\{4}\|U\x\{8}\)\)'\)\@!/ end=/[[:cntrl:][:space:][:punct:]]\@=\|$/ contains=rustSigil,rustLifetime
syn region rustGenericRegion display start=/<\%('\|[^[cntrl:][:space:][:punct:]]\)\@=')\S\@=/ end=/>/ contains=rustGenericLifetimeCandidate
syn region rustGenericLifetimeCandidate display start=/\%(<\|,\s*\)\@<='/ end=/[[:cntrl:][:space:][:punct:]]\@=\|$/ contains=rustSigil,rustLifetime

"rustLifetime must appear before rustCharacter, or chars will get the lifetime highlighting
syn match     rustLifetime    display "\'\%([^[:cntrl:][:space:][:punct:][:digit:]]\|_\)\%([^[:cntrl:][:punct:][:space:]]\|_\)*"
syn match   rustCharacter   /'\([^'\\]\|\\\([nrt\\'"]\|x\x\{2}\|u\x\{4}\|U\x\{8}\)\)'/ contains=rustSpecial

syn region    rustCommentML   start="/\*" end="\*/" contains=rustTodo
syn region    rustComment     start="//" skip="\\$" end="$" contains=rustTodo keepend
syn region    rustCommentMLDoc start="/\*\%(!\|\*/\@!\)" end="\*/" contains=rustTodo
syn region    rustCommentDoc  start="//[/!]" skip="\\$" end="$" contains=rustTodo keepend

syn keyword rustTodo contained TODO FIXME XXX NB NOTE

" Trivial folding rules to begin with.
" TODO: use the AST to make really good folding
syn region rustFoldBraces start="{" end="}" transparent fold
" If you wish to enable this, setlocal foldmethod=syntax
" It's not enabled by default as it would drive some people mad.

hi def link rustHexNumber       rustNumber
hi def link rustBinNumber       rustNumber
hi def link rustIdentifierPrime rustIdentifier
hi def link rustTrait           rustType

hi def link rustSigil         StorageClass
hi def link rustFormat        Special
hi def link rustSpecial       Special
hi def link rustString        String
hi def link rustCharacter     Character
hi def link rustNumber        Number
hi def link rustBoolean       Boolean
hi def link rustConstant      Constant
hi def link rustSelf          Constant
hi def link rustFloat         Float
hi def link rustOperator      Operator
hi def link rustKeyword       Keyword
hi def link rustConditional   Conditional
hi def link rustIdentifier    Identifier
hi def link rustCapsIdent     rustIdentifier
hi def link rustModPath       Include
hi def link rustModPathSep    Delimiter
hi def link rustFuncName      Function
hi def link rustFuncCall      Function
hi def link rustCommentMLDoc  rustCommentDoc
hi def link rustCommentDoc    SpecialComment
hi def link rustCommentML     rustComment
hi def link rustComment       Comment
hi def link rustAssert        PreCondit
hi def link rustFail          PreCondit
hi def link rustMacro         Macro
hi def link rustType          Type
hi def link rustTodo          Todo
hi def link rustAttribute     PreProc
hi def link rustDeriving      PreProc
hi def link rustStorage       StorageClass
hi def link rustLifetime      Special

" Other Suggestions:
" hi rustAttribute ctermfg=cyan
" hi rustDeriving ctermfg=cyan
" hi rustAssert ctermfg=yellow
" hi rustFail ctermfg=red
" hi rustMacro ctermfg=magenta

syn sync minlines=200
syn sync maxlines=500

let b:current_syntax = "rust"
