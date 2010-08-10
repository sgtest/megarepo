
open Common;;
open Token;;
open Parser;;

(* NB: pexps (parser-expressions) are only used transiently during
 * parsing, static-evaluation and syntax-expansion.  They're desugared
 * into the general "item" AST and/or evaluated as part of the
 * outermost "cexp" expressions. Expressions that can show up in source
 * correspond to this loose grammar and have a wide-ish flexibility in
 * *theoretical* composition; only subsets of those compositions are
 * legal in various AST contexts.
 * 
 * Desugaring on the fly is unfortunately complicated enough to require
 * -- or at least "make much more convenient" -- this two-pass
 * routine.
 *)

type pexp' =
    PEXP_call of (pexp * pexp array)
  | PEXP_spawn of (Ast.domain * string * pexp)
  | PEXP_bind of (pexp * pexp option array)
  | PEXP_rec of ((Ast.ident * Ast.mutability * pexp) array * pexp option)
  | PEXP_tup of ((Ast.mutability * pexp) array)
  | PEXP_vec of Ast.mutability * (pexp array)
  | PEXP_port
  | PEXP_chan of (pexp option)
  | PEXP_binop of (Ast.binop * pexp * pexp)
  | PEXP_lazy_and of (pexp * pexp)
  | PEXP_lazy_or of (pexp * pexp)
  | PEXP_unop of (Ast.unop * pexp)
  | PEXP_lval of plval
  | PEXP_lit of Ast.lit
  | PEXP_str of string
  | PEXP_box of Ast.mutability * pexp
  | PEXP_custom of Ast.name * (pexp array) * (string option)

and plval =
    PLVAL_ident of Ast.ident
  | PLVAL_app of (Ast.ident * (Ast.ty array))
  | PLVAL_ext_name of (pexp * Ast.name_component)
  | PLVAL_ext_pexp of (pexp * pexp)
  | PLVAL_ext_deref of pexp

and pexp = pexp' Common.identified
;;

(* Pexp grammar. Includes names, idents, types, constrs, binops and unops,
   etc. *)

let parse_ident (ps:pstate) : Ast.ident =
  match peek ps with
      IDENT id -> (bump ps; id)
    (* Decay IDX tokens to identifiers if they occur ousdide name paths. *)
    | IDX i -> (bump ps; string_of_tok (IDX i))
    | _ -> raise (unexpected ps)
;;

(* Enforces the restricted pexp grammar when applicable (e.g. after "bind") *)
let check_rstr_start (ps:pstate) : 'a =
  if (ps.pstate_rstr) then
    match peek ps with
        IDENT _ | LPAREN -> ()
      | _ -> raise (unexpected ps)
;;

let rec parse_name_component (ps:pstate) : Ast.name_component =
  match peek ps with
      IDENT id ->
        (bump ps;
         match peek ps with
             LBRACKET ->
               let tys =
                 ctxt "name_component: apply"
                   (bracketed_one_or_more LBRACKET RBRACKET
                      (Some COMMA) parse_ty) ps
               in
                 Ast.COMP_app (id, tys)
           | _ -> Ast.COMP_ident id)

    | IDX i ->
        bump ps;
        Ast.COMP_idx i
    | _ -> raise (unexpected ps)

and parse_name_base (ps:pstate) : Ast.name_base =
  match peek ps with
      IDENT i ->
        (bump ps;
         match peek ps with
             LBRACKET ->
               let tys =
                 ctxt "name_base: apply"
                   (bracketed_one_or_more LBRACKET RBRACKET
                      (Some COMMA) parse_ty) ps
               in
                 Ast.BASE_app (i, tys)
           | _ -> Ast.BASE_ident i)
    | _ -> raise (unexpected ps)

and parse_name_ext (ps:pstate) (base:Ast.name) : Ast.name =
  match peek ps with
      DOT ->
        bump ps;
        let comps = one_or_more DOT parse_name_component ps in
          Array.fold_left (fun x y -> Ast.NAME_ext (x, y)) base comps
    | _ -> base


and parse_name (ps:pstate) : Ast.name =
  let base = Ast.NAME_base (parse_name_base ps) in
  let name = parse_name_ext ps base in
    if Ast.sane_name name
    then name
    else raise (err "malformed name" ps)

and parse_carg_base (ps:pstate) : Ast.carg_base =
  match peek ps with
      STAR -> bump ps; Ast.BASE_formal
    | _ -> Ast.BASE_named (parse_name_base ps)

and parse_carg (ps:pstate) : Ast.carg =
  match peek ps with
      IDENT _ | STAR ->
        begin
          let base = Ast.CARG_base (parse_carg_base ps) in
          let path =
            match peek ps with
                DOT ->
                  bump ps;
                  let comps = one_or_more DOT parse_name_component ps in
                    Array.fold_left
                      (fun x y -> Ast.CARG_ext (x, y)) base comps
              | _ -> base
          in
            Ast.CARG_path path
        end
    | _ ->
        Ast.CARG_lit (parse_lit ps)


and parse_constraint (ps:pstate) : Ast.constr =
  match peek ps with

      (*
       * NB: A constraint *looks* a lot like an EXPR_call, but is restricted
       * syntactically: the constraint name needs to be a name (not an lval)
       * and the constraint args all need to be cargs, which are similar to
       * names but can begin with the 'formal' base anchor '*'.
       *)

      IDENT _ ->
        let n = ctxt "constraint: name" parse_name ps in
        let args = ctxt "constraint: args"
          (bracketed_zero_or_more
             LPAREN RPAREN (Some COMMA)
             parse_carg) ps
        in
          { Ast.constr_name = n;
            Ast.constr_args = args }
    | _ -> raise (unexpected ps)


and parse_constrs (ps:pstate) : Ast.constrs =
  ctxt "state: constraints" (one_or_more COMMA parse_constraint) ps

and parse_optional_trailing_constrs (ps:pstate) : Ast.constrs =
  match peek ps with
      COLON -> (bump ps; parse_constrs ps)
    | _ -> [| |]

and parse_effect (ps:pstate) : Ast.effect =
  match peek ps with
      IO -> bump ps; Ast.IO
    | STATE -> bump ps; Ast.STATE
    | UNSAFE -> bump ps; Ast.UNSAFE
    | _ -> Ast.PURE

and parse_mutability (ps:pstate) : Ast.mutability =
  match peek ps with
      MUTABLE -> bump ps; Ast.MUT_mutable
    | _ -> Ast.MUT_immutable

and parse_ty_fn
    (effect:Ast.effect)
    (ps:pstate)
    : (Ast.ty_fn * Ast.ident option) =
  match peek ps with
      FN | ITER ->
        let is_iter = (peek ps) = ITER in
          bump ps;
          let ident =
            match peek ps with
                IDENT i -> bump ps; Some i
              | _ -> None
          in
          let in_slots =
            match peek ps with
                _ ->
                  bracketed_zero_or_more LPAREN RPAREN (Some COMMA)
                    (parse_slot_and_optional_ignored_ident true) ps
          in
          let out_slot =
            match peek ps with
                RARROW -> (bump ps; parse_slot false ps)
              | _ -> slot_nil
          in
          let constrs = parse_optional_trailing_constrs ps in
          let tsig = { Ast.sig_input_slots = in_slots;
                       Ast.sig_input_constrs = constrs;
                       Ast.sig_output_slot = out_slot; }
          in
          let taux = { Ast.fn_effect = effect;
                       Ast.fn_is_iter = is_iter; }
          in
          let tfn = (tsig, taux) in
            (tfn, ident)

    | _ -> raise (unexpected ps)

and check_dup_rec_labels ps labels =
  arr_check_dups labels
    (fun l _ ->
       raise (err (Printf.sprintf
                     "duplicate record label: %s" l) ps));


and parse_atomic_ty (ps:pstate) : Ast.ty =
  match peek ps with

      BOOL ->
        bump ps;
        Ast.TY_bool

    | INT ->
        bump ps;
        Ast.TY_int

    | UINT ->
        bump ps;
        Ast.TY_uint

    | CHAR ->
        bump ps;
        Ast.TY_char

    | STR ->
        bump ps;
        Ast.TY_str

    | ANY ->
        bump ps;
        Ast.TY_any

    | TASK ->
        bump ps;
        Ast.TY_task

    | CHAN ->
        bump ps;
        Ast.TY_chan (bracketed LBRACKET RBRACKET parse_ty ps)

    | PORT ->
        bump ps;
        Ast.TY_port (bracketed LBRACKET RBRACKET parse_ty ps)

    | VEC ->
        bump ps;
        Ast.TY_vec (bracketed LBRACKET RBRACKET parse_ty ps)

    | IDENT _ -> Ast.TY_named (parse_name ps)

    | TAG ->
        bump ps;
        let htab = Hashtbl.create 4 in
        let parse_tag_entry ps =
          let ident = parse_ident ps in
          let tup =
            match peek ps with
                LPAREN -> paren_comma_list parse_ty ps
              | _ -> raise (err "tag variant missing argument list" ps)
          in
            htab_put htab (Ast.NAME_base (Ast.BASE_ident ident)) tup
        in
        let _ =
          bracketed_one_or_more LPAREN RPAREN
            (Some COMMA) (ctxt "tag: variant" parse_tag_entry) ps
        in
          Ast.TY_tag htab

    | REC ->
        bump ps;
        let parse_rec_entry ps =
          let (ty, ident) = parse_ty_and_ident ps in
            (ident, ty)
        in
        let entries = paren_comma_list parse_rec_entry ps in
        let labels = Array.map (fun (l, _) -> l) entries in
          begin
            check_dup_rec_labels ps labels;
            Ast.TY_rec entries
          end

    | TUP ->
        bump ps;
        let tys = paren_comma_list parse_ty ps in
          Ast.TY_tup tys

    | MACH m ->
        bump ps;
        Ast.TY_mach m

    | IO | STATE | UNSAFE | OBJ | FN | ITER ->
        let effect = parse_effect ps in
          begin
            match peek ps with
                OBJ ->
                  bump ps;
                  let methods = Hashtbl.create 0 in
                  let parse_method ps =
                    let effect = parse_effect ps in
                    let (tfn, ident) = parse_ty_fn effect ps in
                      expect ps SEMI;
                      match ident with
                          None ->
                            raise (err (Printf.sprintf
                                          "missing method identifier") ps)
                        | Some i -> htab_put methods i tfn
                  in
                    ignore (bracketed_zero_or_more LBRACE RBRACE
                              None parse_method ps);
                    Ast.TY_obj (effect, methods)

              | FN | ITER ->
                  Ast.TY_fn (fst (parse_ty_fn effect ps))
              | _ -> raise (unexpected ps)
          end

    | AT ->
        bump ps;
        Ast.TY_box (parse_ty ps)

    | MUTABLE ->
        bump ps;
        Ast.TY_mutable (parse_ty ps)

    | LPAREN ->
        begin
          bump ps;
          match peek ps with
              RPAREN ->
                bump ps;
                Ast.TY_nil
            | _ ->
                let t = parse_ty ps in
                  expect ps RPAREN;
                  t
        end

    | _ -> raise (unexpected ps)

and flag (ps:pstate) (tok:token) : bool =
  if peek ps = tok
  then (bump ps; true)
  else false

and parse_slot (aliases_ok:bool) (ps:pstate) : Ast.slot =
  let mode =
  match (peek ps, aliases_ok) with
      (AND, true) -> bump ps; Ast.MODE_alias
    | (AND, false) -> raise (err "alias slot in prohibited context" ps)
    | _ -> Ast.MODE_local
  in
  let ty = parse_ty ps in
    { Ast.slot_mode = mode;
      Ast.slot_ty = Some ty }

and parse_slot_and_ident
    (aliases_ok:bool)
    (ps:pstate)
    : (Ast.slot * Ast.ident) =
  let slot = ctxt "slot and ident: slot" (parse_slot aliases_ok) ps in
  let ident = ctxt "slot and ident: ident" parse_ident ps in
    (slot, ident)

and parse_ty_and_ident
    (ps:pstate)
    : (Ast.ty * Ast.ident) =
  let ty = ctxt "ty and ident: ty" parse_ty ps in
  let ident = ctxt "ty and ident: ident" parse_ident ps in
    (ty, ident)

and parse_slot_and_optional_ignored_ident
    (aliases_ok:bool)
    (ps:pstate)
    : Ast.slot =
  let slot = parse_slot aliases_ok ps in
    begin
      match peek ps with
          IDENT _ -> bump ps
        | _ -> ()
    end;
    slot

and parse_identified_slot
    (aliases_ok:bool)
    (ps:pstate)
    : Ast.slot identified =
  let apos = lexpos ps in
  let slot = parse_slot aliases_ok ps in
  let bpos = lexpos ps in
    span ps apos bpos slot

and parse_constrained_ty (ps:pstate) : Ast.ty =
  let base = ctxt "ty: base" parse_atomic_ty ps in
    match peek ps with
        COLON ->
          bump ps;
          let constrs = ctxt "ty: constrs" parse_constrs ps in
            Ast.TY_constrained (base, constrs)

      | _ -> base

and parse_ty (ps:pstate) : Ast.ty =
  parse_constrained_ty ps


and parse_rec_input (ps:pstate) : (Ast.ident * Ast.mutability * pexp) =
  let mutability = parse_mutability ps in
  let lab = (ctxt "rec input: label" parse_ident ps) in
    match peek ps with
        EQ ->
          bump ps;
          let pexp = ctxt "rec input: expr" parse_pexp ps in
            (lab, mutability, pexp)
      | _ -> raise (unexpected ps)


and parse_rec_body (ps:pstate) : pexp' = (*((Ast.ident * pexp) array) =*)
  begin
    expect ps LPAREN;
    match peek ps with
        RPAREN -> PEXP_rec ([||], None)
      | WITH -> raise (err "empty record extension" ps)
      | _ ->
          let inputs = one_or_more COMMA parse_rec_input ps in
          let labels = Array.map (fun (l, _, _) -> l) inputs in
            begin
              check_dup_rec_labels ps labels;
              match peek ps with
                  RPAREN -> (bump ps; PEXP_rec (inputs, None))
                | WITH ->
                    begin
                      bump ps;
                      let base =
                        ctxt "rec input: extension base"
                          parse_pexp ps
                      in
                        expect ps RPAREN;
                        PEXP_rec (inputs, Some base)
                    end
                | _ -> raise (err "expected 'with' or ')'" ps)
            end
  end


and parse_lit (ps:pstate) : Ast.lit =
  match peek ps with
      LIT_INT i -> (bump ps; Ast.LIT_int i)
    | LIT_UINT i -> (bump ps; Ast.LIT_uint i)
    | LIT_MACH_INT (tm, i) -> (bump ps; Ast.LIT_mach_int (tm, i))
    | LIT_CHAR c -> (bump ps; Ast.LIT_char c)
    | LIT_BOOL b -> (bump ps; Ast.LIT_bool b)
    | _ -> raise (unexpected ps)


and parse_bottom_pexp (ps:pstate) : pexp =
  check_rstr_start ps;
  let apos = lexpos ps in
  match peek ps with

      AT ->
        bump ps;
        let mutability = parse_mutability ps in
        let inner = parse_pexp ps in
        let bpos = lexpos ps in
          span ps apos bpos (PEXP_box (mutability, inner))

    | TUP ->
        bump ps;
        let pexps =
          ctxt "paren pexps(s)" (rstr false parse_mutable_and_pexp_list) ps
        in
        let bpos = lexpos ps in
          span ps apos bpos (PEXP_tup pexps)

    | REC ->
          bump ps;
          let body = ctxt "rec pexp: rec body" parse_rec_body ps in
          let bpos = lexpos ps in
            span ps apos bpos body

    | VEC ->
        bump ps;
        let mutability =
          match peek ps with
              LBRACKET ->
                bump ps;
                expect ps MUTABLE;
                expect ps RBRACKET;
                Ast.MUT_mutable
            | _ -> Ast.MUT_immutable
        in
        let pexps = ctxt "vec pexp: exprs" parse_pexp_list ps in
        let bpos = lexpos ps in
          span ps apos bpos (PEXP_vec (mutability, pexps))


    | LIT_STR s ->
        bump ps;
        let bpos = lexpos ps in
          span ps apos bpos (PEXP_str s)

    | PORT ->
        begin
            bump ps;
            expect ps LPAREN;
            expect ps RPAREN;
            let bpos = lexpos ps in
              span ps apos bpos (PEXP_port)
        end

    | CHAN ->
        begin
            bump ps;
            let port =
              match peek ps with
                  LPAREN ->
                    begin
                      bump ps;
                      match peek ps with
                          RPAREN -> (bump ps; None)
                        | _ ->
                            let lv = parse_pexp ps in
                              expect ps RPAREN;
                              Some lv
                    end
                | _ -> raise (unexpected ps)
            in
            let bpos = lexpos ps in
              span ps apos bpos (PEXP_chan port)
        end

    | SPAWN ->
        bump ps;
        let domain =
          match peek ps with
              THREAD -> bump ps; Ast.DOMAIN_thread
            | _ -> Ast.DOMAIN_local
        in
          (* Spawns either have an explicit literal string for the spawned
             task's name, or the task is named as the entry call
             expression. *)
        let explicit_name =
          match peek ps with
              LIT_STR s -> bump ps; Some s
            | _ -> None
        in
        let pexp =
          ctxt "spawn [domain] [name] pexp: init call" parse_pexp ps
        in
        let bpos = lexpos ps in
        let name =
          match explicit_name with
              Some s -> s
                (* FIXME: string_of_span returns a string like
                   "./driver.rs:10:16 - 11:52", not the actual text at those
                   characters *)
            | None -> Session.string_of_span { lo = apos; hi = bpos }
        in
          span ps apos bpos (PEXP_spawn (domain, name, pexp))

    | BIND ->
        let apos = lexpos ps in
          begin
            bump ps;
            let pexp = ctxt "bind pexp: function" (rstr true parse_pexp) ps in
            let args =
              ctxt "bind args"
                (paren_comma_list parse_bind_arg) ps
            in
            let bpos = lexpos ps in
              span ps apos bpos (PEXP_bind (pexp, args))
          end

    | IDENT i ->
        begin
          bump ps;
          match peek ps with
              LBRACKET ->
                begin
                  let tys =
                    ctxt "apply-type expr"
                      (bracketed_one_or_more LBRACKET RBRACKET
                         (Some COMMA) parse_ty) ps
                  in
                  let bpos = lexpos ps in
                    span ps apos bpos (PEXP_lval (PLVAL_app (i, tys)))
                end

            | _ ->
                begin
                  let bpos = lexpos ps in
                    span ps apos bpos (PEXP_lval (PLVAL_ident i))
                end
        end


    | STAR ->
        bump ps;
        let inner = parse_pexp ps in
        let bpos = lexpos ps in
          span ps apos bpos (PEXP_lval (PLVAL_ext_deref inner))

    | POUND ->
        bump ps;
        let name = parse_name ps in
        let args =
          match peek ps with
              LPAREN ->
                parse_pexp_list ps
            | _ -> [| |]
        in
        let str =
          match peek ps with
              LBRACE ->
                begin
                  bump_bracequote ps;
                  match peek ps with
                      BRACEQUOTE s -> bump ps; Some s
                    | _ -> raise (unexpected ps)
                end
            | _ -> None
        in
        let bpos = lexpos ps in
          span ps apos bpos
            (PEXP_custom (name, args, str))

    | LPAREN ->
        begin
          bump ps;
          match peek ps with
              RPAREN ->
                bump ps;
                let bpos = lexpos ps in
                  span ps apos bpos (PEXP_lit Ast.LIT_nil)
            | _ ->
                let pexp = parse_pexp ps in
                  expect ps RPAREN;
                  pexp
        end

    | _ ->
        let lit = parse_lit ps in
        let bpos = lexpos ps in
          span ps apos bpos (PEXP_lit lit)


and parse_bind_arg (ps:pstate) : pexp option =
  match peek ps with
      UNDERSCORE -> (bump ps; None)
    | _ -> Some (parse_pexp ps)


and parse_ext_pexp (ps:pstate) (pexp:pexp) : pexp =
  let apos = lexpos ps in
    match peek ps with
        LPAREN ->
          if ps.pstate_rstr
          then pexp
          else
            let args = parse_pexp_list ps in
            let bpos = lexpos ps in
            let ext = span ps apos bpos (PEXP_call (pexp, args)) in
              parse_ext_pexp ps ext

      | DOT ->
          begin
            bump ps;
            let ext =
              match peek ps with
                  LPAREN ->
                    bump ps;
                    let rhs = rstr false parse_pexp ps in
                      expect ps RPAREN;
                      let bpos = lexpos ps in
                        span ps apos bpos
                          (PEXP_lval (PLVAL_ext_pexp (pexp, rhs)))
                | _ ->
                    let rhs = parse_name_component ps in
                    let bpos = lexpos ps in
                      span ps apos bpos
                        (PEXP_lval (PLVAL_ext_name (pexp, rhs)))
            in
              parse_ext_pexp ps ext
          end

      | _ -> pexp


and parse_negation_pexp (ps:pstate) : pexp =
    let apos = lexpos ps in
      match peek ps with
          NOT ->
            bump ps;
            let rhs = ctxt "negation pexp" parse_negation_pexp ps in
            let bpos = lexpos ps in
              span ps apos bpos (PEXP_unop (Ast.UNOP_not, rhs))

        | TILDE ->
            bump ps;
            let rhs = ctxt "negation pexp" parse_negation_pexp ps in
            let bpos = lexpos ps in
              span ps apos bpos (PEXP_unop (Ast.UNOP_bitnot, rhs))

        | MINUS ->
            bump ps;
            let rhs = ctxt "negation pexp" parse_negation_pexp ps in
            let bpos = lexpos ps in
              span ps apos bpos (PEXP_unop (Ast.UNOP_neg, rhs))

        | _ ->
            let lhs = parse_bottom_pexp ps in
              parse_ext_pexp ps lhs


(* Binops are all left-associative,                *)
(* so we factor out some of the parsing code here. *)
and binop_build
    (ps:pstate)
    (name:string)
    (apos:pos)
    (rhs_parse_fn:pstate -> pexp)
    (lhs:pexp)
    (step_fn:pexp -> pexp)
    (op:Ast.binop)
    : pexp =
  bump ps;
  let rhs = (ctxt (name ^ " rhs") rhs_parse_fn ps) in
  let bpos = lexpos ps in
  let node = span ps apos bpos (PEXP_binop (op, lhs, rhs)) in
    step_fn node


and parse_factor_pexp (ps:pstate) : pexp =
  let name = "factor pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_negation_pexp ps in
  let build = binop_build ps name apos parse_negation_pexp in
  let rec step accum =
    match peek ps with
        STAR    -> build accum step Ast.BINOP_mul
      | SLASH   -> build accum step Ast.BINOP_div
      | PERCENT -> build accum step Ast.BINOP_mod
      | _       -> accum
  in
    step lhs


and parse_term_pexp (ps:pstate) : pexp =
  let name = "term pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_factor_pexp ps in
  let build = binop_build ps name apos parse_factor_pexp in
  let rec step accum =
    match peek ps with
        PLUS  -> build accum step Ast.BINOP_add
      | MINUS -> build accum step Ast.BINOP_sub
      | _     -> accum
  in
    step lhs


and parse_shift_pexp (ps:pstate) : pexp =
  let name = "shift pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_term_pexp ps in
  let build = binop_build ps name apos parse_term_pexp in
  let rec step accum =
    match peek ps with
        LSL -> build accum step Ast.BINOP_lsl
      | LSR -> build accum step Ast.BINOP_lsr
      | ASR -> build accum step Ast.BINOP_asr
      | _   -> accum
  in
    step lhs


and parse_and_pexp (ps:pstate) : pexp =
  let name = "and pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_shift_pexp ps in
  let build = binop_build ps name apos parse_shift_pexp in
  let rec step accum =
    match peek ps with
        AND -> build accum step Ast.BINOP_and
      | _   -> accum
  in
    step lhs


and parse_xor_pexp (ps:pstate) : pexp =
  let name = "xor pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_and_pexp ps in
  let build = binop_build ps name apos parse_and_pexp in
  let rec step accum =
    match peek ps with
        CARET -> build accum step Ast.BINOP_xor
      | _     -> accum
  in
    step lhs


and parse_or_pexp (ps:pstate) : pexp =
  let name = "or pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_xor_pexp ps in
  let build = binop_build ps name apos parse_xor_pexp in
  let rec step accum =
    match peek ps with
        OR -> build accum step Ast.BINOP_or
      | _  -> accum
  in
    step lhs


and parse_as_pexp (ps:pstate) : pexp =
  let apos = lexpos ps in
  let pexp = ctxt "as pexp" parse_or_pexp ps in
  let rec step accum =
    match peek ps with
        AS ->
          bump ps;
          let tapos = lexpos ps in
          let t = parse_ty ps in
          let bpos = lexpos ps in
          let t = span ps tapos bpos t in
          let node =
            span ps apos bpos
              (PEXP_unop ((Ast.UNOP_cast t), accum))
          in
            step node

      | _ -> accum
  in
    step pexp


and parse_relational_pexp (ps:pstate) : pexp =
  let name = "relational pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_as_pexp ps in
  let build = binop_build ps name apos parse_as_pexp in
  let rec step accum =
    match peek ps with
        LT -> build accum step Ast.BINOP_lt
      | LE -> build accum step Ast.BINOP_le
      | GE -> build accum step Ast.BINOP_ge
      | GT -> build accum step Ast.BINOP_gt
      | _  -> accum
  in
    step lhs


and parse_equality_pexp (ps:pstate) : pexp =
  let name = "equality pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_relational_pexp ps in
  let build = binop_build ps name apos parse_relational_pexp in
  let rec step accum =
    match peek ps with
        EQEQ -> build accum step Ast.BINOP_eq
      | NE   -> build accum step Ast.BINOP_ne
      | _    -> accum
  in
    step lhs


and parse_andand_pexp (ps:pstate) : pexp =
  let name = "andand pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_equality_pexp ps in
  let rec step accum =
    match peek ps with
        ANDAND ->
          bump ps;
          let rhs = parse_equality_pexp ps in
          let bpos = lexpos ps in
          let node = span ps apos bpos (PEXP_lazy_and (accum, rhs)) in
            step node

      | _   -> accum
  in
    step lhs


and parse_oror_pexp (ps:pstate) : pexp =
  let name = "oror pexp" in
  let apos = lexpos ps in
  let lhs = ctxt (name ^ " lhs") parse_andand_pexp ps in
  let rec step accum =
    match peek ps with
        OROR ->
          bump ps;
          let rhs = parse_andand_pexp ps in
          let bpos = lexpos ps in
          let node = span ps apos bpos (PEXP_lazy_or (accum, rhs)) in
            step node

      | _  -> accum
  in
    step lhs


and parse_pexp (ps:pstate) : pexp =
  parse_oror_pexp ps

and parse_mutable_and_pexp (ps:pstate) : (Ast.mutability * pexp) =
  let mutability = parse_mutability ps in
  (mutability, parse_as_pexp ps)

and parse_pexp_list (ps:pstate) : pexp array =
  match peek ps with
      LPAREN ->
        bracketed_zero_or_more LPAREN RPAREN (Some COMMA)
          (ctxt "pexp list" parse_pexp) ps
    | _ -> raise (unexpected ps)

and parse_mutable_and_pexp_list (ps:pstate) : (Ast.mutability * pexp) array =
  match peek ps with
      LPAREN ->
        bracketed_zero_or_more LPAREN RPAREN (Some COMMA)
          (ctxt "mutable-and-pexp list" parse_mutable_and_pexp) ps
    | _ -> raise (unexpected ps)

;;

(* 
 * FIXME: This is a crude approximation of the syntax-extension system,
 * for purposes of prototyping and/or hard-wiring any extensions we
 * wish to use in the bootstrap compiler. The eventual aim is to permit
 * loading rust crates to process extensions, but this will likely
 * require a rust-based frontend, or an ocaml-FFI-based connection to
 * rust crates. At the moment we have neither.
 *)

let expand_pexp_custom
    (ps:pstate)
    (dst_lval:Ast.lval)
    (name:Ast.name)
    (args:Ast.atom array)
    (body:string option)
    (spanner:'a -> 'a identified)
    : (Ast.stmt array) =
  let nstr = Fmt.fmt_to_str Ast.fmt_name name in
    match (nstr, (Array.length args), body) with

        ("shell", 0, Some cmd) ->
          let c = Unix.open_process_in cmd in
          let b = Buffer.create 32 in
          let rec r _ =
            try
              Buffer.add_char b (input_char c);
              r ()
            with
                End_of_file ->
                  ignore (Unix.close_process_in c);
                  Buffer.contents b
          in
            [| spanner (Ast.STMT_new_str (dst_lval, r())) |]

      | _ ->
          raise (err ("unknown syntax extension: " ^ nstr) ps)
;;

(* 
 * Desugarings depend on context:
 * 
 *   - If a pexp is used on the RHS of an assignment, it's turned into
 *     an initialization statement such as STMT_new_rec or such. This
 *     removes the possibility of initializing into a temp only to
 *     copy out. If the topmost pexp in such a desugaring is an atom,
 *     unop or binop, of course, it will still just emit a STMT_copy
 *     on a primitive expression.
 * 
 *   - If a pexp is used in the context where an atom is required, a 
 *     statement declaring a temporary and initializing it with the 
 *     result of the pexp is prepended, and the temporary atom is used.
 *)

let rec desugar_lval (ps:pstate) (pexp:pexp) : (Ast.stmt array * Ast.lval) =
  let s = Hashtbl.find ps.pstate_sess.Session.sess_spans pexp.id in
  let (apos, bpos) = (s.lo, s.hi) in
    match pexp.node with

        PEXP_lval (PLVAL_ident ident) ->
          let nb = span ps apos bpos (Ast.BASE_ident ident) in
            ([||], Ast.LVAL_base nb)

      | PEXP_lval (PLVAL_app (ident, tys)) ->
          let nb = span ps apos bpos (Ast.BASE_app (ident, tys)) in
            ([||], Ast.LVAL_base nb)

      | PEXP_lval (PLVAL_ext_name (base_pexp, comp)) ->
          let (base_stmts, base_atom) = desugar_expr_atom ps base_pexp in
          let base_lval = atom_lval ps base_atom in
            (base_stmts, Ast.LVAL_ext (base_lval, Ast.COMP_named comp))

      | PEXP_lval (PLVAL_ext_pexp (base_pexp, ext_pexp)) ->
          let (base_stmts, base_atom) = desugar_expr_atom ps base_pexp in
          let (ext_stmts, ext_atom) = desugar_expr_atom ps ext_pexp in
          let base_lval = atom_lval ps base_atom in
            (Array.append base_stmts ext_stmts,
             Ast.LVAL_ext (base_lval, Ast.COMP_atom (clone_atom ps ext_atom)))

      | PEXP_lval (PLVAL_ext_deref base_pexp) ->
          let (base_stmts, base_atom) = desugar_expr_atom ps base_pexp in
          let base_lval = atom_lval ps base_atom in
            (base_stmts, Ast.LVAL_ext (base_lval, Ast.COMP_deref))

      | _ ->
          let (stmts, atom) = desugar_expr_atom ps pexp in
            (stmts, atom_lval ps atom)


and desugar_expr
    (ps:pstate)
    (pexp:pexp)
    : (Ast.stmt array * Ast.expr) =
  match pexp.node with

      PEXP_unop (op, pe) ->
        let (stmts, at) = desugar_expr_atom ps pe in
          (stmts, Ast.EXPR_unary (op, at))

    | PEXP_binop (op, lhs, rhs) ->
          let (lhs_stmts, lhs_atom) = desugar_expr_atom ps lhs in
          let (rhs_stmts, rhs_atom) = desugar_expr_atom ps rhs in
            (Array.append lhs_stmts rhs_stmts,
             Ast.EXPR_binary (op, lhs_atom, rhs_atom))

    | _ ->
        let (stmts, at) = desugar_expr_atom ps pexp in
          (stmts, Ast.EXPR_atom at)


and desugar_opt_expr_atom
    (ps:pstate)
    (po:pexp option)
    : (Ast.stmt array * Ast.atom option) =
  match po with
      None -> ([| |], None)
    | Some pexp ->
        let (stmts, atom) = desugar_expr_atom ps pexp in
          (stmts, Some atom)


and desugar_expr_atom
    (ps:pstate)
    (pexp:pexp)
    : (Ast.stmt array * Ast.atom) =
  let s = Hashtbl.find ps.pstate_sess.Session.sess_spans pexp.id in
  let (apos, bpos) = (s.lo, s.hi) in
    match pexp.node with

        PEXP_unop _
      | PEXP_binop _
      | PEXP_lazy_or _
      | PEXP_lazy_and _
      | PEXP_rec _
      | PEXP_tup _
      | PEXP_str _
      | PEXP_vec _
      | PEXP_port
      | PEXP_chan _
      | PEXP_call _
      | PEXP_bind _
      | PEXP_spawn _
      | PEXP_custom _
      | PEXP_box _ ->
          let (_, tmp, decl_stmt) = build_tmp ps slot_auto apos bpos in
          let stmts = desugar_expr_init ps tmp pexp in
            (Array.append [| decl_stmt |] stmts,
             Ast.ATOM_lval (clone_lval ps tmp))

      | PEXP_lit lit ->
          ([||], Ast.ATOM_literal (span ps apos bpos lit))

      | PEXP_lval _ ->
          let (stmts, lval) = desugar_lval ps pexp in
            (stmts, Ast.ATOM_lval lval)

and desugar_expr_atoms
    (ps:pstate)
    (pexps:pexp array)
    : (Ast.stmt array * Ast.atom array) =
  arj1st (Array.map (desugar_expr_atom ps) pexps)

and desugar_opt_expr_atoms
    (ps:pstate)
    (pexps:pexp option array)
    : (Ast.stmt array * Ast.atom option array) =
  arj1st (Array.map (desugar_opt_expr_atom ps) pexps)

and desugar_expr_init
    (ps:pstate)
    (dst_lval:Ast.lval)
    (pexp:pexp)
    : (Ast.stmt array) =
  let s = Hashtbl.find ps.pstate_sess.Session.sess_spans pexp.id in
  let (apos, bpos) = (s.lo, s.hi) in

  (* Helpers. *)
  let ss x = span ps apos bpos x in
  let cp v = Ast.STMT_copy (clone_lval ps dst_lval, v) in
  let aa x y = Array.append x y in
  let ac xs = Array.concat xs in

    match pexp.node with

        PEXP_lit _
      | PEXP_lval _ ->
          let (stmts, atom) = desugar_expr_atom ps pexp in
            aa stmts [| ss (cp (Ast.EXPR_atom atom)) |]

      | PEXP_binop (op, lhs, rhs) ->
          let (lhs_stmts, lhs_atom) = desugar_expr_atom ps lhs in
          let (rhs_stmts, rhs_atom) = desugar_expr_atom ps rhs in
          let copy_stmt =
            ss (cp (Ast.EXPR_binary (op, lhs_atom, rhs_atom)))
          in
            ac [ lhs_stmts; rhs_stmts; [| copy_stmt |] ]

      (* x = a && b ==> if (a) { x = b; } else { x = false; } *)

      | PEXP_lazy_and (lhs, rhs) ->
          let (lhs_stmts, lhs_atom) = desugar_expr_atom ps lhs in
          let (rhs_stmts, rhs_atom) = desugar_expr_atom ps rhs in
          let sthen =
            ss (aa rhs_stmts [| ss (cp (Ast.EXPR_atom rhs_atom)) |])
          in
          let selse =
            ss [| ss (cp (Ast.EXPR_atom
                            (Ast.ATOM_literal (ss (Ast.LIT_bool false))))) |]
          in
          let sif =
            ss (Ast.STMT_if { Ast.if_test = Ast.EXPR_atom lhs_atom;
                              Ast.if_then = sthen;
                              Ast.if_else = Some selse })
          in
            aa lhs_stmts [| sif |]

      (* x = a || b ==> if (a) { x = true; } else { x = b; } *)

      | PEXP_lazy_or (lhs, rhs) ->
          let (lhs_stmts, lhs_atom) = desugar_expr_atom ps lhs in
          let (rhs_stmts, rhs_atom) = desugar_expr_atom ps rhs in
          let sthen =
            ss [| ss (cp (Ast.EXPR_atom
                            (Ast.ATOM_literal (ss (Ast.LIT_bool true))))) |]
          in
          let selse =
            ss (aa rhs_stmts [| ss (cp (Ast.EXPR_atom rhs_atom)) |])
          in
          let sif =
            ss (Ast.STMT_if { Ast.if_test = Ast.EXPR_atom lhs_atom;
                              Ast.if_then = sthen;
                              Ast.if_else = Some selse })
          in
            aa lhs_stmts [| sif |]


      | PEXP_unop (op, rhs) ->
          let (rhs_stmts, rhs_atom) = desugar_expr_atom ps rhs in
          let expr = Ast.EXPR_unary (op, rhs_atom) in
          let copy_stmt = ss (cp expr) in
            aa rhs_stmts [| copy_stmt |]

      | PEXP_call (fn, args) ->
          let (fn_stmts, fn_atom) = desugar_expr_atom ps fn in
          let (arg_stmts, arg_atoms) = desugar_expr_atoms ps args in
          let fn_lval = atom_lval ps fn_atom in
          let call_stmt = ss (Ast.STMT_call (dst_lval, fn_lval, arg_atoms)) in
            ac [ fn_stmts; arg_stmts; [| call_stmt |] ]

      | PEXP_bind (fn, args) ->
          let (fn_stmts, fn_atom) = desugar_expr_atom ps fn in
          let (arg_stmts, arg_atoms) = desugar_opt_expr_atoms ps args in
          let fn_lval = atom_lval ps fn_atom in
          let bind_stmt = ss (Ast.STMT_bind (dst_lval, fn_lval, arg_atoms)) in
            ac [ fn_stmts; arg_stmts; [| bind_stmt |] ]

      | PEXP_spawn (domain, name, sub) ->
          begin
            match sub.node with
                PEXP_call (fn, args) ->
                  let (fn_stmts, fn_atom) = desugar_expr_atom ps fn in
                  let (arg_stmts, arg_atoms) = desugar_expr_atoms ps args in
                  let fn_lval = atom_lval ps fn_atom in
                  let spawn_stmt =
                    ss (Ast.STMT_spawn
                          (dst_lval, domain, name, fn_lval, arg_atoms))
                  in
                    ac [ fn_stmts; arg_stmts; [| spawn_stmt |] ]
              | _ -> raise (err "non-call spawn" ps)
          end

      | PEXP_rec (args, base) ->
          let (arg_stmts, entries) =
            arj1st
              begin
                Array.map
                  begin
                    fun (ident, mutability, pexp) ->
                      let (stmts, atom) =
                        desugar_expr_atom ps pexp
                      in
                        (stmts, (ident, mutability, atom))
                  end
                  args
              end
          in
            begin
              match base with
                  Some base ->
                    let (base_stmts, base_lval) = desugar_lval ps base in
                    let rec_stmt =
                      ss (Ast.STMT_new_rec
                            (dst_lval, entries, Some base_lval))
                    in
                      ac [ arg_stmts; base_stmts; [| rec_stmt |] ]
                | None ->
                    let rec_stmt =
                      ss (Ast.STMT_new_rec (dst_lval, entries, None))
                    in
                      aa arg_stmts [| rec_stmt |]
            end

      | PEXP_tup args ->
          let muts = Array.to_list (Array.map fst args) in
          let (arg_stmts, arg_atoms) =
            desugar_expr_atoms ps (Array.map snd args)
          in
          let arg_atoms = Array.to_list arg_atoms in
          let tup_args = Array.of_list (List.combine muts arg_atoms) in
          let stmt = ss (Ast.STMT_new_tup (dst_lval, tup_args)) in
            aa arg_stmts [| stmt |]

      | PEXP_str s ->
          let stmt = ss (Ast.STMT_new_str (dst_lval, s)) in
            [| stmt |]

      | PEXP_vec (mutability, args) ->
          let (arg_stmts, arg_atoms) = desugar_expr_atoms ps args in
          let stmt =
            ss (Ast.STMT_new_vec (dst_lval, mutability, arg_atoms))
          in
            aa arg_stmts [| stmt |]

      | PEXP_port ->
          [| ss (Ast.STMT_new_port dst_lval) |]

      | PEXP_chan pexp_opt ->
          let (port_stmts, port_opt) =
            match pexp_opt with
                None -> ([||], None)
              | Some port_pexp ->
                  begin
                    let (port_stmts, port_atom) =
                      desugar_expr_atom ps port_pexp
                    in
                    let port_lval = atom_lval ps port_atom in
                      (port_stmts, Some port_lval)
                  end
          in
          let chan_stmt =
            ss
              (Ast.STMT_new_chan (dst_lval, port_opt))
          in
            aa port_stmts [| chan_stmt |]

      | PEXP_box (mutability, arg) ->
          let (arg_stmts, arg_mode_atom) =
            desugar_expr_atom ps arg
          in
          let stmt =
            ss (Ast.STMT_new_box (dst_lval, mutability, arg_mode_atom))
          in
            aa arg_stmts [| stmt |]

      | PEXP_custom (n, a, b) ->
          let (arg_stmts, args) = desugar_expr_atoms ps a in
          let stmts =
            expand_pexp_custom ps dst_lval n args b ss
          in
            aa arg_stmts stmts


and atom_lval (ps:pstate) (at:Ast.atom) : Ast.lval =
  match at with
      Ast.ATOM_lval lv -> lv
    | Ast.ATOM_literal _ -> raise (err "literal where lval expected" ps)
;;




(*
 * Local Variables:
 * fill-column: 78;
 * indent-tabs-mode: nil
 * buffer-file-coding-system: utf-8-unix
 * compile-command: "make -k -C ../.. 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
 * End:
 *)
