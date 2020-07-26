use crate::error::{Error, ErrorCode};

use core::slice::Iter;
use core::str;

use crate::error;
use core::convert::TryFrom;

use crate::expression::{channel_list, numeric_list};

/// SCPI tokens
///
///
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Token<'a> {
    /// Defined as \<mnemonic separator\> consisting of a single `:` character
    HeaderMnemonicSeparator, //:
    HeaderCommonPrefix, //*
    HeaderQuerySuffix,  //?

    ProgramMessageUnitSeparator, //;
    ProgramMessageTerminator,    //\n+END
    ProgramHeaderSeparator,      //SP
    ProgramDataSeparator,        //,
    ProgramMnemonic(&'a [u8]),   // <program mnemonic>
    CharacterProgramData(&'a [u8]),
    DecimalNumericProgramData(f32),
    SuffixProgramData(&'a [u8]),
    NonDecimalNumericProgramData(u32),
    StringProgramData(&'a [u8]),
    ArbitraryBlockData(&'a [u8]),
    ExpressionProgramData(&'a [u8]),
    Utf8BlockData(&'a str),
}

pub enum NumericValues<'a> {
    Maximum,
    Minimum,
    Default,
    Up,
    Down,
    Numeric(Token<'a>),
}

impl<'a> Token<'a> {
    pub fn is_data_object(&self) -> bool {
        match self {
            Token::CharacterProgramData(_)
            | Token::DecimalNumericProgramData(_)
            | Token::SuffixProgramData(_)
            | Token::NonDecimalNumericProgramData(_)
            | Token::StringProgramData(_)
            | Token::ArbitraryBlockData(_) => true,
            _ => false,
        }
    }

    pub(crate) fn mnemonic_split_index(mnemonic: &'a [u8]) -> Option<(&'a [u8], &'a [u8])> {
        let last = mnemonic.iter().rposition(|p| !p.is_ascii_digit());

        if let Some(index) = last {
            if index == mnemonic.len() - 1 {
                None
            } else {
                Some(mnemonic.split_at(index + 1))
            }
        } else {
            None
        }
    }

    /// Compare a string to a mnemonic
    /// # Arguments
    /// * `mnemonic` - A mnemonic to compare with (Example `TRIGger2`)
    /// * `s` - String to compare to mnemonic
    ///
    pub fn mnemonic_compare(mnemonic: &[u8], s: &[u8]) -> bool {
        //LONGform == longform || LONG == long
        //TODO: This sucks.
        let mut optional = true;
        mnemonic.len() >= s.len() && {
            let mut s_iter = s.iter();
            mnemonic.iter().all(|m| {
                let x = s_iter.next();
                if m.is_ascii_lowercase() && x.is_some() {
                    optional = false;
                }
                x.map_or(
                    !(m.is_ascii_uppercase() || m.is_ascii_digit()) && optional,
                    |x| m.eq_ignore_ascii_case(x),
                )
            })
        }
    }

    //TOKen<digits> = [TOK|TOKEN](digits>1)
    pub fn eq_mnemonic(&self, mnemonic: &'a [u8]) -> bool {
        match self {
            Token::ProgramMnemonic(s) | Token::CharacterProgramData(s) => {
                Self::mnemonic_compare(mnemonic, s)
                    || match (
                        Self::mnemonic_split_index(mnemonic),
                        Self::mnemonic_split_index(s),
                    ) {
                        (None, None) => false,
                        (Some((m, index)), None) => Self::mnemonic_compare(m, s) && index == b"1",
                        (None, Some((x, index))) => {
                            Self::mnemonic_compare(mnemonic, x) && index == b"1"
                        }
                        (Some((m, index1)), Some((x, index2))) => {
                            Self::mnemonic_compare(m, x) && (index1 == index2)
                        }
                    }
            }
            _ => false,
        }
    }

    /// If token is an `ExpressionProgramData`, return a numeric list tokenizer to use.
    /// Otherwise a
    ///
    pub fn numeric_list(&self) -> Result<numeric_list::Tokenizer, ErrorCode> {
        if let Token::ExpressionProgramData(str) = self {
            Ok(numeric_list::Tokenizer {
                chars: str.iter(),
                expect_num: true,
            })
        } else {
            Err(ErrorCode::DataTypeError)
        }
    }

    pub fn channel_list(&'a self) -> Result<channel_list::Tokenizer, ErrorCode> {
        if let Token::ExpressionProgramData(str) = self {
            channel_list::Tokenizer::new(str).ok_or(ErrorCode::ExecExpressionError)
        } else {
            Err(ErrorCode::DataTypeError)
        }
    }

    /// If token is `CharacterProgramData`, try to map into another token.
    /// If token is not a `CharacterProgramData` the token will be returned as-is
    ///
    /// # Arguments
    /// * `special` - Function which maps a slice (likely a mnemonic) to a new Token (or emit an error)
    ///
    /// # Example
    ///
    /// ```
    /// use core::convert::TryFrom;
    /// use scpi::tokenizer::*;
    /// use scpi::error::{ErrorCode, Error};
    ///
    /// let s = Token::CharacterProgramData(b"potato");
    ///
    /// if let Ok(value) = s.map_character_data(|s| match s {
    ///     x if Token::mnemonic_compare(b"POTato", x) => Ok(Token::DecimalNumericProgramData(5.0)),
    ///     x if Token::mnemonic_compare(b"PINEapple", x) => Ok(Token::DecimalNumericProgramData(1.0)),
    ///     _ => Err(ErrorCode::IllegalParameterValue.into())
    /// }){
    ///     if let Ok(x) = <u8>::try_from(value){
    ///         assert_eq!(x, 5);
    ///     }else{
    ///         assert_eq!(false, true);
    ///     }
    /// }
    ///
    ///
    ///
    /// //assert_eq!(value, Ok(Token::DecimalNumericProgramData(5.0)));
    ///
    /// ```
    pub fn map_character_data<F>(self, special: F) -> Result<Token<'a>, Error>
    where
        F: FnOnce(&[u8]) -> Result<Token<'a>, Error>,
    {
        if let Token::CharacterProgramData(s) = self {
            special(s)
        } else {
            Ok(self)
        }
    }

    /// Handle data as a SCPI <numeric> and convert to R datatype
    ///
    /// **Note: Decimal data is not compared to the maximum/minimum value and must be done
    /// seperately (unless equal to max/min of that datatype).
    ///
    /// # Example
    /// ```
    /// use scpi::tokenizer::Token;
    /// use scpi::tokenizer::NumericValues;
    /// use scpi::error::ErrorCode;
    /// let  s = Token::CharacterProgramData(b"MAXimum");
    ///
    /// let mut x = 128u8;
    /// if let Ok(v) = s.numeric(|special| match special {
    ///     NumericValues::Maximum => Ok(255u8),
    ///     NumericValues::Minimum => Ok(0u8),
    ///     NumericValues::Default => Ok(1u8),
    ///     NumericValues::Up => Ok(x+1),
    ///     NumericValues::Down => Ok(x-1),
    ///     _ => Err(ErrorCode::ParameterError.into())
    /// }) {
    ///     //v is resolved to a u8 type
    ///     x = v;
    /// }
    ///
    /// ```
    ///
    ///
    pub fn numeric<F, R: TryFrom<Token<'a>, Error = error::Error>>(self, num: F) -> Result<R, Error>
    where
        F: FnOnce(NumericValues) -> Result<R, Error>,
    {
        match self {
            Token::CharacterProgramData(special) => match special {
                x if Token::mnemonic_compare(b"MAXimum", x) => num(NumericValues::Maximum),
                x if Token::mnemonic_compare(b"MINimum", x) => num(NumericValues::Minimum),
                x if Token::mnemonic_compare(b"DEFault", x) => num(NumericValues::Default),
                x if Token::mnemonic_compare(b"UP", x) => num(NumericValues::Up),
                x if Token::mnemonic_compare(b"DOWN", x) => num(NumericValues::Down),
                _ => <R>::try_from(self),
            },
            _ => <R>::try_from(self),
        }
    }

    /// Identical to `numeric()` but enforces min/max values.
    ///
    pub fn numeric_range<F, R: TryFrom<Token<'a>, Error = error::Error>>(
        self,
        min: R,
        max: R,
        num: F,
    ) -> Result<R, Error>
    where
        F: FnOnce(NumericValues) -> Result<R, Error>,
        R: PartialOrd,
    {
        let value = self.numeric(num)?;
        if value > max || value < min {
            Err(ErrorCode::DataOutOfRange.into())
        } else {
            Ok(value)
        }
    }
}

/// Convert string data data into a f32.
///
/// # Returns
/// * `Ok(f32)` - If data is a numeric or a any special value below:
///     - `INFinity` - Returns f32::INFINITY
///     - `NINFinity` - Returns f32::NEG_INIFINTY
///     - `NAN` - Returns f32::NAN
/// * `Err(DataTypeError)` - If data is not a numeric or invalid special value.
/// * `Err(SyntaxError)` - If token is not data
impl<'a> TryFrom<Token<'a>> for f32 {
    type Error = error::Error;

    fn try_from(value: Token) -> Result<Self, Self::Error> {
        match value {
            Token::DecimalNumericProgramData(value) => Ok(value),
            Token::CharacterProgramData(s) => match s {
                //Check for special float values
                ref x if Token::mnemonic_compare(b"INFinity", x) => Ok(f32::INFINITY),
                ref x if Token::mnemonic_compare(b"NINFinity", x) => Ok(f32::NEG_INFINITY),
                ref x if Token::mnemonic_compare(b"NAN", x) => Ok(f32::NAN),
                _ => Err(ErrorCode::DataTypeError.into()),
            },
            Token::SuffixProgramData(_)
            | Token::NonDecimalNumericProgramData(_)
            | Token::StringProgramData(_)
            | Token::ArbitraryBlockData(_) => Err(ErrorCode::DataTypeError.into()),
            _ => Err(ErrorCode::SyntaxError.into()),
        }
    }
}

/// Convert string data data into a slice (&\[u8\]).
///
/// # Returns
/// * `Ok(&[u8])` - If data is a string.
/// * `Err(DataTypeError)` - If data is not a string.
/// * `Err(SyntaxError)` - If token is not data
impl<'a> TryFrom<Token<'a>> for &'a [u8] {
    type Error = error::Error;

    fn try_from(value: Token<'a>) -> Result<&'a [u8], Self::Error> {
        match value {
            Token::StringProgramData(s) => Ok(s),
            Token::SuffixProgramData(_)
            | Token::NonDecimalNumericProgramData(_)
            | Token::DecimalNumericProgramData(_)
            | Token::ArbitraryBlockData(_)
            | Token::CharacterProgramData(_) => Err(ErrorCode::DataTypeError.into()),
            _ => Err(ErrorCode::SyntaxError.into()),
        }
    }
}

/// Convert string data data into a str.
///
/// # Returns
/// * `Ok(&str)` - If data is a string.
/// * `Err(DataTypeError)` - If data is not a string.
/// * `Err(SyntaxError)` - If token is not data
impl<'a> TryFrom<Token<'a>> for &'a str {
    type Error = error::Error;

    fn try_from(value: Token<'a>) -> Result<&'a str, Self::Error> {
        match value {
            Token::StringProgramData(s) | Token::ArbitraryBlockData(s) => {
                str::from_utf8(s).map_err(|_| ErrorCode::StringDataError.into())
            }
            Token::Utf8BlockData(s) => Ok(s),
            Token::SuffixProgramData(_)
            | Token::NonDecimalNumericProgramData(_)
            | Token::DecimalNumericProgramData(_)
            | Token::CharacterProgramData(_) => Err(ErrorCode::DataTypeError.into()),
            _ => Err(ErrorCode::SyntaxError.into()),
        }
    }
}

pub struct Arbitrary<'a>(pub &'a [u8]);
impl<'a> TryFrom<Token<'a>> for Arbitrary<'a> {
    type Error = error::Error;

    fn try_from(value: Token<'a>) -> Result<Arbitrary<'a>, Self::Error> {
        match value {
            Token::StringProgramData(_)
            | Token::SuffixProgramData(_)
            | Token::NonDecimalNumericProgramData(_)
            | Token::DecimalNumericProgramData(_)
            | Token::Utf8BlockData(_)
            | Token::CharacterProgramData(_) => Err(ErrorCode::DataTypeError.into()),
            Token::ArbitraryBlockData(s) => Ok(Arbitrary(s)),
            _ => Err(ErrorCode::SyntaxError.into()),
        }
    }
}

macro_rules! impl_tryfrom_integer {
    ($from:ty) => {
        impl<'a> TryFrom<Token<'a>> for $from {
            type Error = error::Error;

            fn try_from(value: Token) -> Result<Self, Self::Error> {
                #[allow(unused_imports)]
                use crate::lexical_core::Float;
                match value {
                    Token::DecimalNumericProgramData(value) => {
                        if value.is_finite() {
                            <$from>::try_from(value.round() as i64)
                                .map_err(|_| ErrorCode::DataOutOfRange.into())
                        } else {
                            // Nan/Inf -> Integer is undefined, return DataOutOfRange
                            Err(ErrorCode::DataOutOfRange.into())
                        }
                    }
                    Token::NonDecimalNumericProgramData(value) => {
                        <$from>::try_from(value).map_err(|_| ErrorCode::DataOutOfRange.into())
                    }
                    Token::SuffixProgramData(_)
                    | Token::CharacterProgramData(_)
                    | Token::StringProgramData(_)
                    | Token::ArbitraryBlockData(_) => Err(ErrorCode::DataTypeError.into()),
                    _ => Err(ErrorCode::SyntaxError.into()),
                }
            }
        }
    };
}

impl_tryfrom_integer!(i32);
impl_tryfrom_integer!(u32);
impl_tryfrom_integer!(i16);
impl_tryfrom_integer!(u16);
impl_tryfrom_integer!(i8);
impl_tryfrom_integer!(u8);
impl_tryfrom_integer!(usize);
impl_tryfrom_integer!(isize);

#[derive(Clone)]
pub struct Tokenizer<'a> {
    chars: Iter<'a, u8>,
    in_header: bool,
    in_common: bool,
    in_numeric: bool,
}

impl<'a> Tokenizer<'a> {
    /// Attempts to consume a data separator
    /// Returns an error if next token is not a separator.
    /// If next token is a terminator or end,
    /// no error will be returned and token will not be consumed
    pub fn next_separator(&mut self) -> Result<(), ErrorCode> {
        if let Some(item) = self.clone().next() {
            //Check if next item is a data object
            let token = item?;
            match token {
                Token::ProgramDataSeparator => {
                    //Valid data object, consume and return
                    self.next();
                    Ok(())
                }
                Token::ProgramMessageUnitSeparator | Token::ProgramMessageTerminator => {
                    //Message separator or terminator is also ok but do not consume
                    Ok(())
                }
                Token::SuffixProgramData(_) => Err(ErrorCode::SuffixNotAllowed),
                _ => Err(ErrorCode::InvalidSeparator),
            }
        } else {
            //No more tokens, end of message
            Ok(())
        }
    }

    /// Attempts to consume a data object.
    /// If no data is found, none if returned if optional=true else Error:MissingParam.
    ///
    /// Note! Does not skip
    pub fn next_data(&mut self, optional: bool) -> Result<Option<Token<'a>>, ErrorCode> {
        //Try to read a data object
        if let Some(item) = self.clone().next() {
            //Check if next item is a data object
            let token = item?;
            match token {
                //Data object
                Token::CharacterProgramData(_)
                | Token::DecimalNumericProgramData(_)
                | Token::SuffixProgramData(_)
                | Token::NonDecimalNumericProgramData(_)
                | Token::StringProgramData(_)
                | Token::ArbitraryBlockData(_)
                | Token::ExpressionProgramData(_) => {
                    //Valid data object, consume and return
                    self.next();
                    Ok(Some(token))
                }
                //Data separator, next token must be a data object
                Token::ProgramDataSeparator => {
                    self.next();
                    self.next_data(false)
                }
                _ => {
                    //Not a data object, return nothing
                    if optional {
                        Ok(None)
                    } else {
                        Err(ErrorCode::MissingParameter)
                    }
                }
            }
        } else {
            //No more tokens, return nothing
            if optional {
                Ok(None)
            } else {
                Err(ErrorCode::MissingParameter)
            }
        }
    }

    /// Attempt to read a decimal data object (either a DecimalProgramData or a CharacterProgramData).
    /// This function is usually OK'ed and unwrapped before calling a `to_<type>` or `.special()`
    ///
    /// # Arguments
    /// * `optional` - Object is optional, will return None if data is missing and optional, Err otherwise.
    /// * `suffix` - Function to use to parse suffix (if any). Takes the value and suffix slice.
    ///
    /// # Returns
    /// * Ok(NumericalProgramData) - If a NumericalProgramData is found. If a suffix follows, the value is modified with according to suffix.
    /// * Ok(CharacterProgramData) - If a CharacterProgramData is found. Typically a special value.
    ///
    ///
    pub fn next_decimal<F>(&mut self, optional: bool, suffix: F) -> Result<Option<Token<'a>>, Error>
    where
        F: FnOnce(f32, &[u8]) -> Result<f32, Error>,
    {
        let val = self.next_data(optional)?;
        if let Some(t) = val {
            match t {
                Token::DecimalNumericProgramData(mut val) => {
                    //Check if next token is a suffix and reinterpret the value if so
                    let mut clone = self.clone();
                    if let Some(x) = clone.next() {
                        if let Token::SuffixProgramData(s) = x? {
                            //Found a suffix!
                            self.clone_from(&clone);

                            val = suffix(val, s)?;
                            return Ok(Some(Token::DecimalNumericProgramData(val)));
                        }
                    }
                    //No suffix, return as is
                    Ok(Some(t))
                }
                Token::CharacterProgramData(_) => Ok(Some(t)),
                _ => Err(ErrorCode::MissingParameter.into()),
            }
        } else {
            Ok(None)
        }
    }

    pub fn next_arb(&mut self) -> Result<&'a [u8], Error> {
        if let Some(tok) = self.next() {
            let val = tok?;
            match val {
                Token::ArbitraryBlockData(f) => Ok(f),
                Token::DecimalNumericProgramData(_)
                | Token::NonDecimalNumericProgramData(_)
                | Token::StringProgramData(_)
                | Token::CharacterProgramData(_)
                | Token::SuffixProgramData(_) => Err(ErrorCode::DataTypeError.into()),
                _ => Err(ErrorCode::MissingParameter.into()),
            }
        } else {
            Err(ErrorCode::MissingParameter.into())
        }
    }
}

fn ascii_to_digit(digit: u8, radix: u8) -> Option<u32> {
    let lowercase = digit.to_ascii_lowercase();

    if digit.is_ascii_digit() && digit - b'0' < radix {
        Some((digit - b'0') as u32)
    } else if lowercase.is_ascii_alphabetic() && lowercase - b'a' < radix - 10 {
        Some((lowercase - b'a' + 10) as u32)
    } else {
        None
    }
}

impl<'a> Tokenizer<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Tokenizer {
            chars: buf.iter(),
            in_header: true,
            in_common: false,
            in_numeric: false,
        }
    }

    pub fn empty() -> Self {
        Tokenizer {
            chars: b"".iter(),
            in_header: true,
            in_common: false,
            in_numeric: false,
        }
    }

    /// <program mnemonic>
    /// See IEEE 488.2-1992 7.6.1
    /// Must start with a alphabetic character followed by alphanumeric and '_' characters.
    ///
    /// Returned errors:
    /// * ProgramMnemonicTooLong if suffix is longer than 12 characters
    fn read_mnemonic(&mut self) -> Result<Token<'a>, ErrorCode> {
        let s = self.chars.as_slice();
        let mut len = 0u8;
        while self
            .chars
            .clone()
            .next()
            .map_or(false, |ch| ch.is_ascii_alphanumeric() || *ch == b'_')
        {
            self.chars.next();
            len += 1;
            if len > 12 {
                return Err(ErrorCode::ProgramMnemonicTooLong);
            }
        }
        Ok(Token::ProgramMnemonic(
            &s[0..s.len() - self.chars.as_slice().len()],
        ))
    }

    /// <CHARACTER PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.1
    /// Consists of a single <program mnemonic>
    ///
    /// Returned errors:
    /// * CharacterDataTooLong if suffix is longer than 12 characters
    pub(crate) fn read_character_data(&mut self) -> Result<Token<'a>, ErrorCode> {
        let s = self.chars.as_slice();
        let mut len = 0u8;
        while self
            .chars
            .clone()
            .next()
            .map_or(false, |ch| ch.is_ascii_alphanumeric() || *ch == b'_')
        {
            self.chars.next();
            len += 1;
            if len > 12 {
                return Err(ErrorCode::CharacterDataTooLong);
            }
        }
        let ret = Ok(Token::CharacterProgramData(
            &s[0..s.len() - self.chars.as_slice().len()],
        ));

        // Skip to next separator
        self.skip_ws_to_separator(ErrorCode::InvalidCharacterData)?;
        ret
    }

    /// <DECIMAL NUMERIC PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.2
    ///
    /// TODO: lexical-core does not accept whitespace between exponent separator and exponent <mantissa>E <exponent>.
    pub(crate) fn read_numeric_data(&mut self) -> Result<Token<'a>, ErrorCode> {
        /* Read mantissa */
        let (f, len) = lexical_core::parse_partial::<f32>(self.chars.as_slice()).map_err(|e| {
            match e.code {
                lexical_core::ErrorCode::InvalidDigit => ErrorCode::InvalidCharacterInNumber,
                lexical_core::ErrorCode::Overflow | lexical_core::ErrorCode::Underflow => {
                    ErrorCode::ExponentTooLarge
                }
                _ => ErrorCode::NumericDataError,
            }
        })?;
        self.chars.nth(len - 1).unwrap();
        self.skip_ws();

        Ok(Token::DecimalNumericProgramData(f))
    }

    /// <SUFFIX PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.3
    /// Reads a suffix and returns it as a string if successful, otherwise it returns an error.
    /// TODO: Syntax check suffix
    ///
    /// Returned errors:
    /// * SuffixTooLong if suffix is longer than 12 characters
    fn read_suffix_data(&mut self) -> Result<Token<'a>, ErrorCode> {
        let s = self.chars.as_slice();
        let mut len = 0u8;
        while self.chars.clone().next().map_or(false, |ch| {
            ch.is_ascii_alphanumeric() || *ch == b'-' || *ch == b'/' || *ch == b'.'
        }) {
            self.chars.next();
            len += 1;
            if len > 12 {
                return Err(ErrorCode::SuffixTooLong);
            }
        }

        let ret = Ok(Token::SuffixProgramData(
            &s[0..s.len() - self.chars.as_slice().len()],
        ));
        // Skip to next separator
        self.skip_ws_to_separator(ErrorCode::InvalidSuffix)?;
        ret
    }

    /// <NONDECIMAL NUMERIC PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.4
    /// Reads a non-decimal numeric
    ///
    /// Returned errors:
    fn read_nondecimal_data(&mut self, radix: u8) -> Result<Token<'a>, ErrorCode> {
        let radixi = match radix {
            b'H' | b'h' => 16u32,
            b'Q' | b'q' => 8u32,
            b'B' | b'b' => 2u32,
            _ => return Err(ErrorCode::NumericDataError),
        };

        let mut acc = 0u32;
        let mut any = false;
        while self
            .chars
            .clone()
            .next()
            .map_or(false, |ch| ch.is_ascii_alphanumeric())
        {
            let c = self.chars.next().unwrap();
            acc = ascii_to_digit(*c, radixi as u8).ok_or(ErrorCode::InvalidCharacterInNumber)?
                + (acc * radixi);
            any = true;
        }
        if !any {
            return Err(ErrorCode::NumericDataError);
        }
        let ret = Ok(Token::NonDecimalNumericProgramData(acc));
        // Skip to next separator
        self.skip_ws_to_separator(ErrorCode::InvalidSeparator)?;
        ret
    }

    /// <STRING PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.5
    ///
    pub(crate) fn read_string_data(&mut self, x: u8, ascii: bool) -> Result<Token<'a>, ErrorCode> {
        self.chars.next(); //Consume first
        let s = self.chars.as_slice();
        loop {
            if let Some(c) = self.chars.next() {
                // End quote
                if *c == x {
                    if let Some(c2) = self.chars.clone().next() {
                        if *c2 == x {
                            // Double quote, consume and continue
                            self.chars.next().unwrap();
                            continue;
                        } else {
                            // Single quote
                            break;
                        }
                    }
                    break;
                }

                //Only ASCII allowed
                if ascii && !c.is_ascii() {
                    return Err(ErrorCode::InvalidCharacter);
                }
            } else {
                // Unexpected terminator
                return Err(ErrorCode::InvalidStringData);
            }
        }
        let ret = Ok(Token::StringProgramData(
            &s[0..s.len() - self.chars.as_slice().len() - 1],
        ));
        // Skip to next separator
        self.skip_ws_to_separator(ErrorCode::InvalidSeparator)?;
        ret
    }

    /// <ARBITRARY DATA PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.6
    ///
    fn read_arbitrary_data(&mut self, format: u8) -> Result<Token<'a>, ErrorCode> {
        if let Some(len) = ascii_to_digit(format, 10) {
            if len == 0 {
                //Take rest of string
                let rest = self.chars.as_slice();
                let u8str = rest
                    .get(0..rest.len() - 1)
                    .ok_or(ErrorCode::InvalidBlockData)?;
                for _ in u8str {
                    self.chars.next();
                }
                //Indefinite block data must be terminated with NL before END
                if *self.chars.next().unwrap() != b'\n' {
                    return Err(ErrorCode::InvalidBlockData);
                }
                return Ok(Token::ArbitraryBlockData(u8str));
            }

            let payload_len = lexical_core::parse::<usize>(&self.chars.as_slice()[..len as usize])
                .map_err(|_| ErrorCode::InvalidBlockData)?;
            self.chars.nth(len as usize - 1).unwrap();
            let u8str = self
                .chars
                .as_slice()
                .get(0..payload_len)
                .ok_or(ErrorCode::InvalidBlockData)?;
            for _ in 0..payload_len {
                self.chars.next();
            }

            let ret = Ok(Token::ArbitraryBlockData(u8str));
            // Skip to next separator
            self.skip_ws_to_separator(ErrorCode::InvalidSeparator)?;
            ret
        } else {
            Err(ErrorCode::InvalidBlockData)
        }
    }

    /// <ARBITRARY DATA PROGRAM DATA>
    /// Non standard custom arbitrary data block format #s"..."/#s'...'
    /// The parser will automatically check and convert the data to a UTF8 string, emitting
    /// a InvalidBlockData error if the string is not valid UTF8.
    #[cfg(feature = "arbitrary-utf8-string")]
    fn read_utf8_data(&mut self, _format: u8) -> Result<Token<'a>, ErrorCode> {
        if let Some(x) = self.chars.clone().next() {
            if *x != b'"' && *x != b'\'' {
                return Err(ErrorCode::NoError);
            }
            if let Token::StringProgramData(s) = self.read_string_data(*x, false)? {
                if let Ok(u) = str::from_utf8(s) {
                    Ok(Token::Utf8BlockData(u))
                } else {
                    Err(ErrorCode::InvalidBlockData)
                }
            } else {
                Err(ErrorCode::InvalidBlockData)
            }
        } else {
            Err(ErrorCode::InvalidBlockData)
        }
    }

    /// <EXPRESSION PROGRAM DATA>
    /// See IEEE 488.2-1992 7.7.7
    ///
    pub(crate) fn read_expression_data(&mut self) -> Result<Token<'a>, ErrorCode> {
        self.chars.next();
        let s = self.chars.as_slice();
        static ILLEGAL_CHARS: [u8; 6] = [b')', b'"', b'\'', b';', b'(', b')'];
        //Read until closing ')'
        while self.chars.clone().next().map_or(false, |ch| *ch != b')') {
            let c = self.chars.next().unwrap();
            //Return an error if a unexpected character is encountered
            if ILLEGAL_CHARS.contains(c) || !c.is_ascii() {
                return Err(ErrorCode::InvalidExpression);
            }
        }

        let ret = Ok(Token::ExpressionProgramData(
            &s[0..s.len() - self.chars.as_slice().len()],
        ));
        //Consume ending ')' or throw error if there's none
        if self.chars.next().is_none() {
            return Err(ErrorCode::InvalidExpression);
        }
        // Skip to next separator
        self.skip_ws_to_separator(ErrorCode::InvalidSuffix)?;
        ret
    }

    fn skip_ws(&mut self) {
        while self
            .chars
            .clone()
            .next()
            .map_or(false, |ch| ch.is_ascii_whitespace())
        {
            self.chars.next();
        }
    }

    fn skip_ws_to_separator(&mut self, error: ErrorCode) -> Result<(), ErrorCode> {
        self.skip_ws();
        if let Some(c) = self.chars.clone().next() {
            if *c != b',' && *c != b';' && *c != b'\n' {
                return Err(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test_parse {
    use crate::error::ErrorCode;
    use crate::tokenizer::{Token, Tokenizer};

    extern crate std;

    #[test]
    fn test_split_mnemonic() {
        assert_eq!(
            Token::mnemonic_split_index(b"TRIGger54"),
            Some((b"TRIGger".as_ref(), b"54".as_ref()))
        );
        assert_eq!(
            Token::mnemonic_split_index(b"T123r54"),
            Some((b"T123r".as_ref(), b"54".as_ref()))
        );
        assert_eq!(Token::mnemonic_split_index(b"TRIGger"), None);
        assert_eq!(Token::mnemonic_split_index(b""), None);
    }

    #[test]
    fn test_compare_mnemonic() {
        //Should return true
        assert!(Token::mnemonic_compare(b"TRIGger", b"trigger"));
        assert!(Token::mnemonic_compare(b"TRIGger", b"trig"));
        assert!(Token::mnemonic_compare(b"TRIGger", b"TRIGGER"));
        assert!(Token::mnemonic_compare(b"TRIGger", b"TRIG"));
        //Should return false
        assert!(!Token::mnemonic_compare(b"TRIGger", b"TRIGge"));
        assert!(!Token::mnemonic_compare(
            b"TRIGger",
            b"triggeristoodamnlong"
        ));
        assert!(!Token::mnemonic_compare(b"TRIGger", b"tri"));
    }

    #[test]
    fn test_eq_mnemonic() {
        //
        assert!(Token::ProgramMnemonic(b"trigger").eq_mnemonic(b"TRIGger1"));
        assert!(Token::ProgramMnemonic(b"trig").eq_mnemonic(b"TRIGger1"));
        assert!(Token::ProgramMnemonic(b"trigger1").eq_mnemonic(b"TRIGger1"));
        assert!(Token::ProgramMnemonic(b"trigger1").eq_mnemonic(b"TRIGger"));
        assert!(Token::ProgramMnemonic(b"trig1").eq_mnemonic(b"TRIGger1"));
        assert!(!Token::ProgramMnemonic(b"trigger2").eq_mnemonic(b"TRIGger1"));
        assert!(!Token::ProgramMnemonic(b"trig2").eq_mnemonic(b"TRIGger1"));
        assert!(!Token::ProgramMnemonic(b"trig2").eq_mnemonic(b"TRIGger1"));

        assert!(Token::ProgramMnemonic(b"trigger2").eq_mnemonic(b"TRIGger2"));
        assert!(Token::ProgramMnemonic(b"trig2").eq_mnemonic(b"TRIGger2"));
        assert!(!Token::ProgramMnemonic(b"trigger").eq_mnemonic(b"TRIGger2"));
        assert!(!Token::ProgramMnemonic(b"trig").eq_mnemonic(b"TRIGger2"));
        assert!(!Token::ProgramMnemonic(b"trigger1").eq_mnemonic(b"TRIGger2"));
        assert!(!Token::ProgramMnemonic(b"trig1").eq_mnemonic(b"TRIGger2"));
    }

    #[test]
    fn test_read_character_data() {
        assert_eq!(
            Tokenizer::new(b"CHARacter4 , pperg").read_character_data(),
            Ok(Token::CharacterProgramData(b"CHARacter4"))
        );
        assert_eq!(
            Tokenizer::new(b"CHARacterIsTooLong").read_character_data(),
            Err(ErrorCode::CharacterDataTooLong)
        );
        assert_eq!(
            Tokenizer::new(b"Character Invalid").read_character_data(),
            Err(ErrorCode::InvalidCharacterData)
        );
    }

    #[test]
    fn test_read_numeric_data() {
        //TODO: FIX EXPONENTS!

        assert_eq!(
            Tokenizer::new(b"25").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(25f32)
        );

        assert_eq!(
            Tokenizer::new(b"-10.").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(-10f32)
        );

        assert_eq!(
            Tokenizer::new(b".2").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(0.2f32)
        );

        assert_eq!(
            Tokenizer::new(b"1.E5").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(1e5f32)
        );

        assert_eq!(
            Tokenizer::new(b"-25e5").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(-25e5f32)
        );

        assert_eq!(
            Tokenizer::new(b"25E-2").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(0.25f32)
        );

        assert_eq!(
            Tokenizer::new(b".1E2").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(10f32)
        );

        assert_eq!(
            Tokenizer::new(b".1E2").read_numeric_data().unwrap(),
            Token::DecimalNumericProgramData(10f32)
        );
    }

    #[test]
    fn test_read_suffix_data() {
        assert_eq!(
            Tokenizer::new(b"MOHM").read_suffix_data().unwrap(),
            Token::SuffixProgramData(b"MOHM")
        );

        // Error, too long suffix
        assert_eq!(
            Tokenizer::new(b"SUFFIXTOODAMNLONG").read_suffix_data(),
            Err(ErrorCode::SuffixTooLong)
        );
    }

    #[test]
    fn test_read_numeric_suffix_data() {
        //let mut tokenizer = Tokenizer::from_str(b"header 25 KHZ , 12.7E6 KOHM.M/S-2");
        //        assert_eq!(tokenizer.next(), Some(Ok(Token::ProgramMnemonic(b"header"))));
        //        assert_eq!(tokenizer.next(), Some(Ok(Token::ProgramHeaderSeparator)));
        //        assert_eq!(tokenizer.next_u8(), Ok(25u8));
        //        assert_eq!(tokenizer.next(), Some(Ok(Token::SuffixProgramData(b"KHZ"))));
        //        assert_eq!(tokenizer.next(), Some(Ok(Token::ProgramDataSeparator)));
        //        assert_eq!(tokenizer.next_f32(), Ok(12.7e6f32));
        //        assert_eq!(tokenizer.next(), Some(Ok(Token::SuffixProgramData(b"KOHM.M/S-2"))));
    }

    #[test]
    fn test_read_string_data() {
        assert_eq!(
            Tokenizer::new(b"\"MOHM\",  gui").read_string_data(b'"', true),
            Ok(Token::StringProgramData(b"MOHM"))
        );
        assert_eq!(
            Tokenizer::new(b"'MOHM',  gui").read_string_data(b'\'', true),
            Ok(Token::StringProgramData(b"MOHM"))
        );
        assert_eq!(
            Tokenizer::new(b"'MO''HM',  gui").read_string_data(b'\'', true),
            Ok(Token::StringProgramData(b"MO''HM"))
        );

        assert_eq!(
            Tokenizer::new(b"\"MOHM").read_string_data(b'"', true),
            Err(ErrorCode::InvalidStringData)
        );
        assert_eq!(
            Tokenizer::new(b"'MOHM").read_string_data(b'"', true),
            Err(ErrorCode::InvalidStringData)
        );
        assert_eq!(
            Tokenizer::new(b"'MO\xffHM").read_string_data(b'"', true),
            Err(ErrorCode::InvalidCharacter)
        );
    }

    #[test]
    fn test_read_arb_data() {
        assert_eq!(
            Tokenizer::new(b"02\x01\x02,").read_arbitrary_data(b'2'),
            Ok(Token::ArbitraryBlockData(&[1, 2]))
        );
        assert_eq!(
            Tokenizer::new(b"2\x01\x02,").read_arbitrary_data(b'1'),
            Ok(Token::ArbitraryBlockData(&[1, 2]))
        );

        // Error, too short
        assert_eq!(
            Tokenizer::new(b"02\x01").read_arbitrary_data(b'2'),
            Err(ErrorCode::InvalidBlockData)
        );

        // Error, invalid header
        assert_eq!(
            Tokenizer::new(b"a2\x01\x02,").read_arbitrary_data(b'2'),
            Err(ErrorCode::InvalidBlockData)
        );

        // Error, header too short/invalid
        assert_eq!(
            Tokenizer::new(b"2\x01\x02,").read_arbitrary_data(b'2'),
            Err(ErrorCode::InvalidBlockData)
        );

        // Indefinite length
        assert_eq!(
            Tokenizer::new(b"\x01\x02\n").read_arbitrary_data(b'0'),
            Ok(Token::ArbitraryBlockData(&[1, 2]))
        );

        // Error, indefinite not terminated by newline
        assert_eq!(
            Tokenizer::new(b"\x01\x02").read_arbitrary_data(b'0'),
            Err(ErrorCode::InvalidBlockData)
        );
    }

    #[cfg(feature = "arbitrary-utf8-string")]
    #[test]
    fn test_read_arb_utf8() {
        assert_eq!(
            Tokenizer::new("'åäö'".as_bytes()).read_utf8_data(b's'),
            Ok(Token::Utf8BlockData("åäö"))
        );
        assert_eq!(
            Tokenizer::new("\"åäö\"".as_bytes()).read_utf8_data(b's'),
            Ok(Token::Utf8BlockData("åäö"))
        );
        assert_eq!(
            Tokenizer::new("'å''äö'".as_bytes()).read_utf8_data(b's'),
            Ok(Token::Utf8BlockData("å''äö"))
        );
        //Missing quote
        assert_eq!(
            Tokenizer::new("'å''äö".as_bytes()).read_utf8_data(b's'),
            Err(ErrorCode::InvalidStringData)
        );
        //Invalid utf8
        assert_eq!(
            Tokenizer::new(b"'\xff'").read_utf8_data(b's'),
            Err(ErrorCode::InvalidBlockData)
        );
    }

    #[test]
    fn test_read_expr_data() {
        assert_eq!(
            Tokenizer::new(b"(@1!2,2,3,4,5,#,POTATO)").read_expression_data(),
            Ok(Token::ExpressionProgramData(b"@1!2,2,3,4,5,#,POTATO"))
        );
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Result<Token<'a>, ErrorCode>;

    fn next(&mut self) -> Option<Self::Item> {
        let x = self.chars.clone().next()?;
        match x {
            /* Common command prefix */
            b'*' => {
                self.in_common = true;
                self.chars.next();
                if let Some(x) = self.chars.clone().next() {
                    if !x.is_ascii_alphabetic() {
                        return Some(Err(ErrorCode::CommandHeaderError));
                    }
                }

                /* Not allowed outside header and strings */
                Some(Ok(Token::HeaderCommonPrefix))
            }
            /* Header mnemonic separator/prefix */
            b':' => {
                self.chars.next();
                //Only one separator is allowed
                if let Some(x) = self.chars.clone().next() {
                    if !x.is_ascii_alphabetic() {
                        return Some(Err(ErrorCode::InvalidSeparator));
                    }
                }
                /* Not allowed outside header and strings */
                if !self.in_header || self.in_common {
                    Some(Err(ErrorCode::InvalidSeparator))
                } else {
                    Some(Ok(Token::HeaderMnemonicSeparator))
                }
            }
            /* Header query suffix */
            b'?' => {
                self.chars.next();
                //Next character after query must be a space, unit separator or <END>
                if let Some(x) = self.chars.clone().next() {
                    if !x.is_ascii_whitespace() && *x != b';' {
                        return Some(Err(ErrorCode::InvalidSeparator));
                    }
                }
                if !self.in_header {
                    Some(Err(ErrorCode::InvalidSeparator))
                } else {
                    self.in_header = false;
                    Some(Ok(Token::HeaderQuerySuffix))
                }
            }
            /* Program unit separator */
            b';' => {
                self.chars.next();
                self.skip_ws();
                self.in_header = true;
                self.in_common = false;
                Some(Ok(Token::ProgramMessageUnitSeparator))
            }
            /* Message terminator */
            //END is implied.
            // Parser should reset itself and parse next message as a new message.
            b'\n' => {
                self.chars.next();
                Some(Ok(Token::ProgramMessageTerminator))
            }
            /* Data separator*/
            b',' => {
                self.chars.next();
                if self.in_header {
                    Some(Err(ErrorCode::HeaderSeparatorError))
                } else {
                    self.in_numeric = false;
                    self.skip_ws();
                    if let Some(c) = self.chars.clone().next() {
                        if *c == b',' || *c == b';' || *c == b'\n' {
                            return Some(Err(ErrorCode::SyntaxError));
                        }
                    }
                    Some(Ok(Token::ProgramDataSeparator))
                }
            }
            /* Whitespace */
            // Separates the header from arguments
            x if x.is_ascii_whitespace() => {
                self.skip_ws();
                /* Header ends */
                self.in_header = false;
                Some(Ok(Token::ProgramHeaderSeparator))
            }
            /* Alphabetic */
            x if x.is_ascii_alphabetic() => {
                /* If still parsing header, it's an mnemonic, else character data */
                if self.in_header {
                    Some(self.read_mnemonic())
                } else if self.in_numeric {
                    Some(self.read_suffix_data())
                } else {
                    Some(self.read_character_data())
                }
            }
            /* Suffix starting with '/' */
            b'/' => {
                if self.in_header {
                    Some(Err(ErrorCode::InvalidSeparator))
                } else {
                    Some(self.read_suffix_data())
                }
            }
            /* Number */
            x if x.is_ascii_digit() || *x == b'-' || *x == b'+' || *x == b'.' => {
                if self.in_header {
                    Some(Err(ErrorCode::CommandHeaderError))
                } else {
                    self.in_numeric = true;
                    Some(self.read_numeric_data())
                }
            }
            /* Arb. block or non-decimal data */
            b'#' => {
                self.chars.next();
                if self.in_header {
                    Some(Err(ErrorCode::CommandHeaderError))
                } else if let Some(x) = self.chars.next() {
                    Some(match x {
                        /* Arbitrary block */
                        #[cfg(feature = "arbitrary-utf8-string")]
                        x if *x == b's' => self.read_utf8_data(*x),
                        x if x.is_ascii_digit() => self.read_arbitrary_data(*x),
                        /*Non-decimal numeric*/
                        _ => self.read_nondecimal_data(*x),
                    })
                } else {
                    Some(Err(ErrorCode::BlockDataError))
                }
            }
            /* String */
            x if *x == b'\'' || *x == b'"' => {
                if self.in_header {
                    Some(Err(ErrorCode::CommandHeaderError))
                } else {
                    Some(self.read_string_data(*x, true))
                }
            }
            b'(' => Some(self.read_expression_data()),
            /* Unknown/unexpected */
            _ => {
                let x = self.chars.next().unwrap();
                if x.is_ascii() {
                    Some(Err(ErrorCode::SyntaxError))
                } else {
                    Some(Err(ErrorCode::InvalidCharacter))
                }
            }
        }
    }
}
