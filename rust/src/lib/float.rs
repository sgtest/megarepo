/**
 * String conversions
 */

fn to_str(num: float, digits: uint) -> str {
    let accum = if num < 0.0 { num = -num; "-" } else { "" };
    let trunc = num as uint;
    let frac = num - (trunc as float);
    accum += uint::str(trunc);
    if frac == 0.0 || digits == 0u { ret accum; }
    accum += ".";
    while digits > 0u && frac > 0.0 {
        frac *= 10.0;
        let digit = frac as uint;
        accum += uint::str(digit);
        frac -= digit as float;
        digits -= 1u;
    }
    ret accum;
}

/**
 * Convert a string to a float
 *
 * This function accepts strings such as
 * * "3.14"
 * * "+3.14", equivalent to "3.14"
 * * "-3.14"
 * * "2.5E10", or equivalently, "2.5e10"
 * * "2.5E-10"
 * * "", or, equivalently, "." (understood as 0)
 * * "5."
 * * ".5", or, equivalently,  "0.5"
 *
 * @param num A string, possibly empty.
 * @return [NaN] if the string did not represent a valid number.
 * @return Otherwise, the floating-point number represented [num].
 */
fn from_str(num: str) -> float {
   let pos = 0u;                  //Current byte position in the string.
                                  //Used to walk the string in O(n).
   let len = str::byte_len(num);  //Length of the string, in bytes.

   if len == 0u { ret 0.; }
   let total = 0f;                //Accumulated result
   let c     = 'z';               //Latest char.

   //Determine if first char is '-'/'+'. Set [pos] and [neg] accordingly.
   let neg = false;               //Sign of the result
   alt str::char_at(num, 0u) {
      '-' {
          neg = true;
          pos = 1u;
      }
      '+' {
          pos = 1u;
      }
      _ {}
   }

   //Examine the following chars until '.', 'e', 'E'
   while(pos < len) {
       let char_range = str::char_range_at(num, pos);
       c   = char_range.ch;
       pos = char_range.next;
       alt c {
          '0' | '1' | '2' | '3' | '4' | '5' | '6'| '7' | '8' | '9'  {
           total = total * 10f;
           total += ((c as int) - ('0' as int)) as float;
         }
         _ {
           break;
         }
       }
   }

   if c == '.' {//Examine decimal part
      let decimal = 1.f;
      while(pos < len) {
         let char_range = str::char_range_at(num, pos);
         c = char_range.ch;
         pos = char_range.next;
         alt c {
            '0' | '1' | '2' | '3' | '4' | '5' | '6'| '7' | '8' | '9'  {
                 decimal /= 10.f;
                 total += (((c as int) - ('0' as int)) as float)*decimal;
             }
             _ {
                 break;
             }
         }
      }
   }

   if (c == 'e') | (c == 'E') {//Examine exponent
      let exponent = 0u;
      let neg_exponent = false;
      if(pos < len) {
          let char_range = str::char_range_at(num, pos);
          c   = char_range.ch;
          alt c  {
             '+' {
                pos = char_range.next;
             }
             '-' {
                pos = char_range.next;
                neg_exponent = true;
             }
             _ {}
          }
          while(pos < len) {
             let char_range = str::char_range_at(num, pos);
             c = char_range.ch;
             pos = char_range.next;
             alt c {
                 '0' | '1' | '2' | '3' | '4' | '5' | '6'| '7' | '8' | '9' {
                     exponent *= 10u;
                     exponent += ((c as uint) - ('0' as uint));
                 }
                 _ {
                     break;
                 }
             }
          }
          let multiplier = pow_uint_to_uint_as_float(10u, exponent);
              //Note: not [int::pow], otherwise, we'll quickly
              //end up with a nice overflow
          if neg_exponent {
             total = total / multiplier;
          } else {
             total = total * multiplier;
          }
      }
   }

   if(pos < len) {
     ret NaN();
   } else {
     if(neg) {
        total *= -1f;
     }
     ret total;
   }
}

/**
 * Arithmetics
 */

/**
 * Compute the exponentiation of an integer by another integer as a float.
 *
 *
 * @param x The base.
 * @param pow The exponent.
 * @return [NaN] of both [x] and [pow] are [0u], otherwise [x^pow].
 */
fn pow_uint_to_uint_as_float(x: uint, pow: uint) -> float {
   if x == 0u {
      if pow == 0u {
        ret NaN();
      }
       ret 0.;
   }
   let my_pow     = pow;
   let total      = 1f;
   let multiplier = x as float;
   while (my_pow > 0u) {
     if my_pow % 2u == 1u {
       total = total * multiplier;
     }
     my_pow     /= 2u;
     multiplier *= multiplier;
   }
   ret total;
}


/**
 * Constants
 */

//TODO: Once this is possible, replace the body of these functions
//by an actual constant.

fn NaN() -> float {
   ret 0./0.;
}

fn infinity() -> float {
   ret 1./0.;
}

fn neg_infinity() -> float {
   ret -1./0.;
}

//
// Local Variables:
// mode: rust
// fill-column: 78;
// indent-tabs-mode: nil
// c-basic-offset: 4
// buffer-file-coding-system: utf-8-unix
// compile-command: "make -k -C $RBUILD 2>&1 | sed -e 's/\\/x\\//x:\\//g'";
// End:
//
