import util::common::ty_mach;
import util::common::ty_mach_to_str;
import util::common::new_str_hash;
import std::_int;
import std::_uint;
import std::_str;

type str_num = uint;

tag binop {
    PLUS;
    MINUS;
    STAR;
    SLASH;
    PERCENT;
    CARET;
    AND;
    OR;
    LSL;
    LSR;
    ASR;
}

tag token {
    /* Expression-operator symbols. */
    EQ;
    LT;
    LE;
    EQEQ;
    NE;
    GE;
    GT;
    ANDAND;
    OROR;
    NOT;
    TILDE;

    BINOP(binop);
    BINOPEQ(binop);

    /* Structural symbols */
    AT;
    DOT;
    COMMA;
    SEMI;
    COLON;
    MOD_SEP;
    QUES;
    RARROW;
    SEND;
    LARROW;
    LPAREN;
    RPAREN;
    LBRACKET;
    RBRACKET;
    LBRACE;
    RBRACE;

    POUND;

    /* Literals */
    LIT_INT(int);
    LIT_UINT(uint);
    LIT_MACH_INT(ty_mach, int);
    LIT_FLOAT(str_num);
    LIT_MACH_FLOAT(ty_mach, str_num);
    LIT_STR(str_num);
    LIT_CHAR(char);
    LIT_BOOL(bool);

    /* Name components */
    IDENT(str_num);
    IDX(int);
    UNDERSCORE;

    BRACEQUOTE(str_num);
    EOF;
}

fn binop_to_str(binop o) -> str {
    alt (o) {
        case (PLUS) { ret "+"; }
        case (MINUS) { ret "-"; }
        case (STAR) { ret "*"; }
        case (SLASH) { ret "/"; }
        case (PERCENT) { ret "%"; }
        case (CARET) { ret "^"; }
        case (AND) { ret "&"; }
        case (OR) { ret "|"; }
        case (LSL) { ret "<<"; }
        case (LSR) { ret ">>"; }
        case (ASR) { ret ">>>"; }
    }
}

fn to_str(lexer::reader r, token t) -> str {
    alt (t) {

        case (EQ) { ret "="; }
        case (LT) { ret "<"; }
        case (LE) { ret "<="; }
        case (EQEQ) { ret "=="; }
        case (NE) { ret "!="; }
        case (GE) { ret ">="; }
        case (GT) { ret ">"; }
        case (NOT) { ret "!"; }
        case (TILDE) { ret "~"; }
        case (OROR) { ret "||"; }
        case (ANDAND) { ret "&&"; }

        case (BINOP(?op)) { ret binop_to_str(op); }
        case (BINOPEQ(?op)) { ret binop_to_str(op) + "="; }

        /* Structural symbols */
        case (AT) { ret "@"; }
        case (DOT) { ret "."; }
        case (COMMA) { ret ","; }
        case (SEMI) { ret ";"; }
        case (COLON) { ret ":"; }
        case (MOD_SEP) { ret "::"; }
        case (QUES) { ret "?"; }
        case (RARROW) { ret "->"; }
        case (SEND) { ret "<|"; }
        case (LARROW) { ret "<-"; }
        case (LPAREN) { ret "("; }
        case (RPAREN) { ret ")"; }
        case (LBRACKET) { ret "["; }
        case (RBRACKET) { ret "]"; }
        case (LBRACE) { ret "{"; }
        case (RBRACE) { ret "}"; }

        case (POUND) { ret "#"; }

        /* Literals */
        case (LIT_INT(?i)) { ret _int::to_str(i, 10u); }
        case (LIT_UINT(?u)) { ret _uint::to_str(u, 10u); }
        case (LIT_MACH_INT(?tm, ?i)) {
            ret  _int::to_str(i, 10u)
                + "_" + ty_mach_to_str(tm);
        }
        case (LIT_MACH_FLOAT(?tm, ?s)) {
            ret r.get_str(s) + "_" + ty_mach_to_str(tm);
        }

        case (LIT_FLOAT(?s)) { ret r.get_str(s); }
        case (LIT_STR(?s)) {
            // FIXME: escape.
            ret "\"" + r.get_str(s) + "\"";
        }
        case (LIT_CHAR(?c)) {
            // FIXME: escape.
            auto tmp = "'";
            _str::push_char(tmp, c);
            _str::push_byte(tmp, '\'' as u8);
            ret tmp;
        }

        case (LIT_BOOL(?b)) {
            if (b) { ret "true"; } else { ret "false"; }
        }

        /* Name components */
        case (IDENT(?s)) {
            ret r.get_str(s);
        }
        case (IDX(?i)) { ret "_" + _int::to_str(i, 10u); }
        case (UNDERSCORE) { ret "_"; }

        case (BRACEQUOTE(_)) { ret "<bracequote>"; }
        case (EOF) { ret "<eof>"; }
    }
}


// Local Variables:
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
