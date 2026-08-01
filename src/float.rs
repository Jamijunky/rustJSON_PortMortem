//! A zero-dependency reimplementation of C's `%g` floating point conversion
//! (as used by `sprintf("%1.15g", ...)` in `cJSON.c`).
//!
//! The conversion is exact: the double is expanded into its exact decimal
//! digits (via big-integer arithmetic over its binary representation) and then
//! rounded to the requested number of significant digits with round-half-even,
//! mirroring the C standard's `%g` semantics.

use core::cmp::Ordering;

/// Render `d` with `%g` semantics and exactly `precision` significant digits,
/// appending the result (without a NUL terminator) to `out`.
pub fn format_g(d: f64, precision: usize, out: &mut Vec<u8>) {
    if d.is_nan() {
        out.extend_from_slice(b"nan");
        return;
    }
    if d.is_infinite() {
        if d < 0.0 {
            out.extend_from_slice(b"-inf");
        } else {
            out.extend_from_slice(b"inf");
        }
        return;
    }

    let neg = d.is_sign_negative();
    let mag = d.abs();

    if mag == 0.0 {
        // %g prints "0" for 0.0 and "-0" for -0.0
        if neg {
            out.extend_from_slice(b"-0");
        } else {
            out.push(b'0');
        }
        return;
    }

    if neg {
        out.push(b'-');
    }

    let (digits, n) = exact_decimal(mag);
    let (sig, n_rounded) = round_significant(&digits, n, precision);
    // X is the decimal exponent of the leading digit after rounding.
    let x = n_rounded - 1;

    if x < -4 || x >= precision as i64 {
        // exponential style (like %e with precision - 1)
        out.push(sig[0]);
        let mut end = precision;
        while end > 1 && sig[end - 1] == b'0' {
            end -= 1;
        }
        if end > 1 {
            out.push(b'.');
            out.extend_from_slice(&sig[1..end]);
        }
        out.push(b'e');
        if x < 0 {
            out.push(b'-');
        } else {
            out.push(b'+');
        }
        write_exponent(out, x.abs());
    } else {
        // fixed style
        if x >= 0 {
            let int_digits = (x + 1) as usize;
            out.extend_from_slice(&sig[..int_digits]);
            let mut end = precision;
            while end > int_digits && sig[end - 1] == b'0' {
                end -= 1;
            }
            if end > int_digits {
                out.push(b'.');
                out.extend_from_slice(&sig[int_digits..end]);
            }
        } else {
            out.push(b'0');
            let mut frac: Vec<u8> = Vec::with_capacity(precision + 8);
            for _ in 0..(x.unsigned_abs() - 1) {
                frac.push(b'0');
            }
            frac.extend_from_slice(&sig);
            while frac.last() == Some(&b'0') {
                frac.pop();
            }
            if !frac.is_empty() {
                out.push(b'.');
                out.extend_from_slice(&frac);
            }
        }
    }
}

/// Decimal digits are zero-filled and the integer magnitude part is dropped
/// for values less than one; helper for the exponent field.
fn write_exponent(out: &mut Vec<u8>, mag: i64) {
    if mag < 10 {
        out.push(b'0');
    }
    let mut buf = [0u8; 20];
    let mut k = buf.len();
    let mut m = mag;
    if m == 0 {
        buf[19] = b'0';
        k = 19;
    }
    while m > 0 {
        k -= 1;
        buf[k] = b'0' + (m % 10) as u8;
        m /= 10;
    }
    out.extend_from_slice(&buf[k..]);
}

/// Produce the exact decimal digits of `d` (no leading zeros) and the exponent
/// `n` such that `d == 0.<digits> * 10^n`. `d` must be finite and non-zero.
fn exact_decimal(d: f64) -> (Vec<u8>, i64) {
    let bits = d.to_bits();
    let exp_bits = ((bits >> 52) & 0x7FF) as i64;
    let frac = bits & 0xF_FFFF_FFFF_FFFF;

    let (m, e): (u64, i64) = if exp_bits == 0 {
        // subnormal: value = frac * 2^-1074
        (frac, -1074)
    } else {
        (frac | (1 << 52), exp_bits - 1023 - 52)
    };

    let mut n = BigU32::from_u64(m);
    let p10: i64;
    if e >= 0 {
        n.shl_bits(e as u32);
        p10 = 0;
    } else {
        let k = (-e) as u32;
        for _ in 0..k {
            n.mul_small(5);
        }
        p10 = e;
    }

    let digits = n.to_decimal_digits();
    let n_val = digits.len() as i64 + p10;
    (digits, n_val)
}

/// Round the exact digit string to `p` significant digits (round half even),
/// returning the `p`-digit result and the updated exponent.
fn round_significant(digits: &[u8], n: i64, p: usize) -> (Vec<u8>, i64) {
    if digits.len() <= p {
        let mut sig = digits.to_vec();
        sig.resize(p, b'0');
        return (sig, n);
    }

    let mut cmp = Ordering::Equal;
    for (i, &d) in digits[p..].iter().enumerate() {
        let h = if i == 0 { b'5' } else { b'0' };
        if d != h {
            cmp = if d > h { Ordering::Greater } else { Ordering::Less };
            break;
        }
    }

    let mut sig: Vec<u8> = digits[..p].to_vec();
    let mut n2 = n;
    match cmp {
        Ordering::Greater => round_up(&mut sig, &mut n2),
        Ordering::Equal => {
            // round half to even
            if sig[p - 1] % 2 == 1 {
                round_up(&mut sig, &mut n2);
            }
        }
        Ordering::Less => {}
    }
    (sig, n2)
}

/// Increment the significant-digit number, propagating carries. If every digit
/// is nine the result is `1` followed by zeros and the exponent grows by one.
fn round_up(sig: &mut Vec<u8>, n: &mut i64) {
    let p = sig.len();
    let mut i = p as isize - 1;
    while i >= 0 {
        if sig[i as usize] < b'9' {
            sig[i as usize] += 1;
            return;
        }
        sig[i as usize] = b'0';
        i -= 1;
    }
    sig.clear();
    sig.push(b'1');
    sig.resize(p, b'0');
    *n += 1;
}

/// A minimal big unsigned integer, stored little-endian in base 2^32.
struct BigU32 {
    limbs: Vec<u32>,
}

impl BigU32 {
    fn from_u64(mut x: u64) -> Self {
        let mut limbs = Vec::new();
        while x > 0 {
            limbs.push((x & 0xFFFF_FFFF) as u32);
            x >>= 32;
        }
        if limbs.is_empty() {
            limbs.push(0);
        }
        BigU32 { limbs }
    }

    fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }

    fn mul_small(&mut self, m: u32) {
        if m == 1 {
            return;
        }
        let mut carry: u64 = 0;
        for l in self.limbs.iter_mut() {
            let cur = (*l as u64) * (m as u64) + carry;
            *l = cur as u32;
            carry = cur >> 32;
        }
        while carry > 0 {
            self.limbs.push(carry as u32);
            carry >>= 32;
        }
    }

    fn shl_bits(&mut self, n: u32) {
        let word = (n / 32) as usize;
        let bits = n % 32;
        if word > 0 {
            let mut new_limbs = vec![0u32; word];
            new_limbs.extend_from_slice(&self.limbs);
            self.limbs = new_limbs;
        }
        if bits > 0 {
            let mut carry: u32 = 0;
            for l in self.limbs.iter_mut() {
                let cur = ((*l as u64) << bits) | carry as u64;
                *l = cur as u32;
                carry = (cur >> 32) as u32;
            }
            if carry > 0 {
                self.limbs.push(carry);
            }
        }
    }

    /// Divide by 10^9 and return the remainder.
    fn divmod_1e9(&mut self) -> u32 {
        let mut rem: u64 = 0;
        for l in self.limbs.iter_mut().rev() {
            let cur = (rem << 32) | (*l as u64);
            *l = (cur / 1_000_000_000) as u32;
            rem = cur % 1_000_000_000;
        }
        while self.limbs.len() > 1 && *self.limbs.last().unwrap() == 0 {
            self.limbs.pop();
        }
        rem as u32
    }

    fn to_decimal_digits(&self) -> Vec<u8> {
        if self.is_zero() {
            return vec![b'0'];
        }
        let mut n = BigU32 {
            limbs: self.limbs.clone(),
        };
        let mut chunks = Vec::new();
        while !n.is_zero() {
            chunks.push(n.divmod_1e9());
        }
        let top = chunks.len() - 1;
        let mut digits = Vec::with_capacity(chunks.len() * 9);
        for i in (0..=top).rev() {
            let c = chunks[i];
            let mut buf = [0u8; 9];
            if i == top {
                let mut x = c;
                let mut k = 9;
                while x > 0 {
                    k -= 1;
                    buf[k] = b'0' + (x % 10) as u8;
                    x /= 10;
                }
                if k == 9 {
                    // c == 0, keep a single zero
                    buf[8] = b'0';
                    k = 8;
                }
                digits.extend_from_slice(&buf[k..]);
            } else {
                let mut x = c;
                let mut k = 9;
                while x > 0 {
                    k -= 1;
                    buf[k] = b'0' + (x % 10) as u8;
                    x /= 10;
                }
                while k > 0 {
                    k -= 1;
                    buf[k] = b'0';
                }
                digits.extend_from_slice(&buf);
            }
        }
        digits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(d: f64, p: usize) -> String {
        let mut out = Vec::new();
        format_g(d, p, &mut out);
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn print_number_test_cases() {
        assert_eq!(g(0.0, 15), "0");
        assert_eq!(g(-1.0, 15), "-1");
        assert_eq!(g(-32768.0, 15), "-32768");
        assert_eq!(g(-2147483648.0, 15), "-2147483648");
        assert_eq!(g(1.0, 15), "1");
        assert_eq!(g(32767.0, 15), "32767");
        assert_eq!(g(2147483647.0, 15), "2147483647");
        assert_eq!(g(0.123, 15), "0.123");
        assert_eq!(g(10e-10, 15), "1e-09");
        assert_eq!(g(10e11, 15), "1000000000000");
        assert_eq!(g(123e127, 15), "1.23e+129");
        assert_eq!(g(123e-128, 15), "1.23e-126");
        assert_eq!(g(3.1415926535897931, 15), "3.14159265358979");
        assert_eq!(g(3.1415926535897931, 17), "3.1415926535897931");
        assert_eq!(g(-0.0123, 15), "-0.0123");
        assert_eq!(g(-10e-10, 15), "-1e-09");
        assert_eq!(g(-10e20, 15), "-1e+21");
        assert_eq!(g(-123e127, 15), "-1.23e+129");
        assert_eq!(g(-123e-128, 15), "-1.23e-126");
    }

    #[test]
    fn rounding_ties_half_even() {
        // 0.125 rounded to 2 significant digits: the trailing 5 is a tie and
        // the last kept digit (2) is even, so it rounds down.
        assert_eq!(g(0.125, 2), "0.12");
        // 0.135: last kept digit 3 is odd, so it rounds up.
        assert_eq!(g(0.135, 2), "0.14");
        // 9.99 rounds to 10 (exponent bumps from 0 to 1).
        assert_eq!(g(9.99, 2), "10");
        // 999.0 with 2 significant digits: exponent 2 >= 2 -> exponential.
        assert_eq!(g(999.0, 2), "1e+03");
    }

    #[test]
    fn extremes() {
        assert_eq!(g(f64::MIN_POSITIVE, 15), "2.2250738585072e-308");
        assert_eq!(g(f64::MAX, 15), "1.79769313486232e+308");
        assert_eq!(g(5e-324, 15), "4.94065645841247e-324");
    }

    #[test]
    fn matches_libc_snprintf() {
        use core::ffi::{c_char, c_int};
        unsafe extern "C" {
            fn snprintf(
                s: *mut c_char,
                n: usize,
                format: *const c_char,
                ...
            ) -> c_int;
        }
        let mut rng = 0x9E3779B97F4A7C15u64;
        for _ in 0..200_000 {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            let bits = rng;
            let d = f64::from_bits(bits);
            for p in [15usize, 17usize] {
                let mut mine = Vec::new();
                format_g(d, p, &mut mine);
                let mut buf = [0 as c_char; 64];
                let fmt_ptr = if p == 15 {
                    b"%1.15g\0".as_ptr() as *const c_char
                } else {
                    b"%1.17g\0".as_ptr() as *const c_char
                };
                unsafe {
                    let _ = snprintf(buf.as_mut_ptr(), buf.len(), fmt_ptr, d);
                }
                let theirs = unsafe { core::ffi::CStr::from_ptr(buf.as_ptr()).to_bytes() };
                assert_eq!(
                    mine,
                    theirs,
                    "mismatch for {:#016x} (d={:e}) precision {}",
                    bits,
                    d,
                    p
                );
            }
        }
    }
}
