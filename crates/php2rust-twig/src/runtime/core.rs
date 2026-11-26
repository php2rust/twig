//! Port of the runtime methods of Twig\Extension\CoreExtension.php: the
//! core filters, functions and tests, `getAttribute()`, and the helpers
//! the compiled templates call (`ensureTraversable()`, `toArray()`,
//! `inFilter()`, `compare()`, `matches()`).
//!
//! Each filter, function and test is a [`crate::runtime::Callable`]: it
//! gets the call and the arguments in PHP order, without the
//! charset, environment and context (they come from the call). PHP's own
//! functions the core extension maps to (`abs`, `json_encode`, `max`,
//! `min`, `range`, `is_iterable`) are here too.
//!
//! Dates (`date`, `date_modify`, the `date` function) use php2rust-date.
//!
//! Not ported yet: `convert_encoding` (iconv),
//! and the sandbox checks (the sandbox extension is not ported).
//!
//! Upstream: twig/twig src/Extension/CoreExtension.php

// The callables share one signature (`fn(&Call, Vec<Value>) -> R`), so
// they take the arguments by value even when they only read them; the
// short names follow the PHP code.
#![allow(clippy::needless_pass_by_value, clippy::many_single_char_names)]

use std::{cmp::Ordering, fmt::Write as _, sync::Arc};

use php_value::{Key, PhpArray, PhpObject, PhpString, Thrown, Value, ops};

use super::{Call, R, string as mb};
use crate::{
    environment::Environment,
    error::{Error, ErrorKind},
    markup::Markup,
    source::Source,
    template::{ARRAY_CALL, METHOD_CALL},
};

/// The options of the core extension: `setDateFormat()`,
/// `setNumberFormat()`, `setTimezone()`.
#[derive(Debug, Clone)]
pub struct CoreOptions {
    pub date_formats: [Vec<u8>; 2],
    /// Decimals, decimal point, thousands separator.
    pub number_format: (i64, Vec<u8>, Vec<u8>),
    pub timezone: Option<String>,
    /// The constants of the application's own classes (`Foo::BAR`), which
    /// `constant()` and `getAttribute()` read after PHP's built-in ones:
    /// Rust has no PHP class constants to look up.
    pub constants: Vec<(Vec<u8>, Value)>,
    /// The application's enums, for `enum()`, `enum_cases()` and their
    /// `Class::CASE` constants.
    pub enums: php_value::constants::Registry,
}

impl Default for CoreOptions {
    fn default() -> Self {
        Self {
            date_formats: [b"F j, Y H:i".to_vec(), b"%d days".to_vec()],
            number_format: (0, b".".to_vec(), b",".to_vec()),
            timezone: None,
            constants: Vec::new(),
            enums: php_value::constants::Registry::empty(),
        }
    }
}

/// `CoreExtension::ARRAY_LIKE_CLASSES`.
pub const ARRAY_LIKE_CLASSES: [&str; 11] = [
    "ArrayIterator",
    "ArrayObject",
    "CachingIterator",
    "RecursiveArrayIterator",
    "RecursiveCachingIterator",
    "SplDoublyLinkedList",
    "SplFixedArray",
    "SplObjectStorage",
    "SplQueue",
    "SplStack",
    "WeakMap",
];

const DEFAULT_TRIM_CHARS: &[u8] = b" \t\n\r\0\x0B";

/// An arrow function of a compiled template, as a `\Closure` value: the
/// environment keeps the function (`Environment::register_arrow`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrow(usize);

impl Arrow {
    #[must_use]
    pub fn id(self) -> usize {
        self.0
    }

    /// The arrow as a value.
    #[must_use]
    pub fn value(id: usize) -> Value {
        Value::Object(Arc::new(Self(id)))
    }
}

impl PhpObject for Arrow {
    fn class(&self) -> &'static str {
        "Closure"
    }

    fn properties(&self) -> PhpArray {
        PhpArray::new()
    }

    fn methods(&self) -> Vec<String> {
        vec![
            "bind".into(),
            "bindTo".into(),
            "call".into(),
            "fromCallable".into(),
        ]
    }
}

fn runtime(message: impl Into<Vec<u8>>) -> Error {
    Error::runtime(message, -1, None)
}

fn type_error(message: impl Into<Vec<u8>>) -> Error {
    Error::exception("TypeError", message)
}

fn value_error(message: impl Into<Vec<u8>>) -> Error {
    Error::exception("ValueError", message)
}

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or(Value::Null)
}

fn arg_or(args: &[Value], i: usize, default: Value) -> Value {
    args.get(i).cloned().unwrap_or(default)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn string_of(value: &Value) -> R<Vec<u8>> {
    super::bytes(value)
}

/// `$value ?? ''` cast to a string.
fn string_or_empty(value: &Value) -> R<Vec<u8>> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    string_of(value)
}

fn count_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// The string of a template name argument (`string|TemplateWrapper|array`
/// parameters: scalars are coerced).
pub fn string_argument(value: &Value) -> R<PhpString> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => super::to_php_string(value),
        Value::Object(o) if o.is_stringable() => super::to_php_string(value),
        other => Err(type_error(format!(
            "Twig\\Template::load(): Argument #1 ($template) must be of type Twig\\TemplateWrapper|array|string, {} given",
            other.debug_type()
        ))),
    }
}

/// `is_iterable($value)`.
#[must_use]
pub fn is_iterable_value(value: &Value) -> bool {
    match value {
        Value::Array(_) => true,
        Value::Object(o) => o.instance_of("Traversable"),
        _ => false,
    }
}

fn is_traversable(value: &Value) -> bool {
    matches!(value, Value::Object(o) if o.instance_of("Traversable"))
}

fn traverse(value: &Value) -> R<Vec<(Value, Value)>> {
    match value {
        Value::Object(o) => match o.traverse() {
            Some(pairs) => Ok(pairs?),
            None => Ok(Vec::new()),
        },
        Value::Array(a) => Ok(a.iter().map(|(k, v)| (k.to_value(), v.clone())).collect()),
        _ => Ok(Vec::new()),
    }
}

fn key_of(key: &Value) -> R<Key> {
    Key::from_value(key).ok_or_else(|| {
        type_error(format!(
            "Cannot access offset of type {} on array",
            key.debug_type()
        ))
    })
}

/// `iterator_to_array($value, $preserveKeys)`.
fn iterator_to_array(value: &Value, preserve_keys: bool) -> R<PhpArray> {
    let mut out = PhpArray::new();
    for (k, v) in traverse(value)? {
        if preserve_keys {
            out.insert(key_of(&k)?, v);
        } else {
            let _ = out.push(v);
        }
    }
    Ok(out)
}

fn values_of(array: &PhpArray) -> PhpArray {
    PhpArray::list(array.values().cloned())
}

/// `ensureTraversable()`.
#[must_use]
pub fn ensure_traversable(seq: Value) -> Value {
    if is_iterable_value(&seq) {
        return seq;
    }
    Value::Array(PhpArray::new())
}

/// `toArray($seq, $preserveKeys)`.
pub fn to_array(seq: &Value, preserve_keys: bool) -> R {
    if is_traversable(seq) {
        return Ok(Value::Array(iterator_to_array(seq, preserve_keys)?));
    }
    match seq {
        Value::Array(a) if !preserve_keys => Ok(Value::Array(values_of(a))),
        other => Ok(other.clone()),
    }
}

/// `toArray()` of a value that must give an array.
fn array_of(seq: &Value, preserve_keys: bool) -> R<PhpArray> {
    match to_array(seq, preserve_keys)? {
        Value::Array(a) => Ok(a),
        other => Ok(php_value::object::to_array(&other)),
    }
}

/// `CoreExtension::compare()`: PHP 8's `<=>` with the string to number
/// comparison rules of PHP 8 applied to PHP 7 too.
#[must_use]
pub fn compare(a: &Value, b: &Value) -> i64 {
    const WS: &[u8] = b" \t\n\r\x0B\x0C";
    let trim = |s: &[u8]| -> Vec<u8> {
        let start = s.iter().position(|c| !WS.contains(c)).unwrap_or(s.len());
        let end = s
            .iter()
            .rposition(|c| !WS.contains(c))
            .map_or(start, |e| e + 1);
        s[start..end.max(start)].to_vec()
    };
    let int_cmp = |x: i64, s: &[u8], swap: bool| -> i64 {
        let t = trim(s);
        let tv = Value::String(PhpString::from(t.clone()));

        if php_value::numeric::is_numeric(&t) {
            let as_int = php_value::numeric::string_to_int(&t);
            if php_value::compare::loose_equals(&Value::Int(as_int), &tv) {
                if swap {
                    as_int.cmp(&x) as i64
                } else {
                    x.cmp(&as_int) as i64
                }
            } else {
                let f = php_value::numeric::string_to_float(&t);
                #[allow(clippy::cast_precision_loss)]
                let xf = x as f64;
                let (l, r) = if swap { (f, xf) } else { (xf, f) };
                l.partial_cmp(&r).map_or(1, |o| o as i64)
            }
        } else {
            let l = Value::String(PhpString::from(x.to_string()));
            let rr = Value::String(PhpString::from(s));
            if swap {
                ops::spaceship(&rr, &l)
            } else {
                ops::spaceship(&l, &rr)
            }
        }
    };
    let float_cmp = |x: f64, s: &[u8], swap: bool| -> i64 {
        if x.is_nan() {
            return 1;
        }
        let t = trim(s);
        if !php_value::numeric::is_numeric(&t) {
            let l = Value::String(PhpString::from(php_value::numeric::float_to_string(x)));
            let rr = Value::String(PhpString::from(s));
            return if swap {
                ops::spaceship(&rr, &l)
            } else {
                ops::spaceship(&l, &rr)
            };
        }
        let f = php_value::numeric::string_to_float(&t);
        let (l, r) = if swap { (f, x) } else { (x, f) };
        l.partial_cmp(&r).map_or(1, |o| o as i64)
    };
    match (a, b) {
        (Value::Int(x), Value::String(s)) => int_cmp(*x, s.as_bytes(), false),
        (Value::String(s), Value::Int(x)) => int_cmp(*x, s.as_bytes(), true),
        (Value::Float(x), Value::String(s)) => float_cmp(*x, s.as_bytes(), false),
        (Value::String(s), Value::Float(x)) => float_cmp(*x, s.as_bytes(), true),
        _ => ops::spaceship(a, b),
    }
}

/// `inFilter($value, $compare)`.
pub fn in_filter(value: &Value, compare_to: &Value) -> R<bool> {
    let unwrap = |v: &Value| -> Value {
        match v {
            Value::Object(o) => match o.downcast_ref::<Markup>() {
                Some(m) => Value::String(m.content().clone()),
                None => v.clone(),
            },
            other => other.clone(),
        }
    };
    let value = unwrap(value);
    let compare_to = unwrap(compare_to);
    if let Value::String(haystack) = &compare_to {
        return Ok(match &value {
            Value::String(_) | Value::Int(_) | Value::Float(_) => {
                let needle = string_of(&value)?;
                needle.is_empty() || contains(haystack.as_bytes(), &needle)
            }
            _ => false,
        });
    }
    if !is_iterable_value(&compare_to) {
        return Ok(false);
    }
    let items = traverse(&compare_to)?;
    if matches!(value, Value::Object(_)) {
        return Ok(items.iter().any(|(_, item)| item.identical(&value)));
    }
    Ok(items.iter().any(|(_, item)| compare(&value, item) == 0))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || haystack.windows(needle.len()).any(|w| w == needle)
}

/// `matches($regexp, $str)`.
pub fn matches(regexp: &Value, subject: &Value) -> R {
    let pattern = string_of(regexp)?;
    let subject = string_or_empty(subject)?;
    let regex = php_pcre::Pcre::new(&pattern).map_err(|e| {
        let (php_pcre::PcreError::Compile(message) | php_pcre::PcreError::Unsupported(message)) = e;
        runtime(format!(
            "Regexp \"{}\" passed to \"matches\" is not valid: {message}",
            text(&pattern)
        ))
    })?;
    let found = regex.is_match(&subject).map_err(|e| {
        runtime(format!(
            "Regexp \"{}\" passed to \"matches\" failed: {}.",
            text(&pattern),
            e.0
        ))
    })?;
    Ok(Value::Int(i64::from(found)))
}

/// `testEmpty($value)`.
pub fn test_empty_value(value: &Value) -> R<bool> {
    if let Value::Object(o) = value {
        if let Some(count) = o.php_count() {
            return Ok(count? == 0);
        }
        if o.instance_of("Traversable") {
            return Ok(traverse(value)?.is_empty());
        }
        if o.is_stringable() {
            return Ok(string_of(value)?.is_empty());
        }
        return Ok(false);
    }
    Ok(match value {
        Value::String(s) => s.is_empty(),
        Value::Bool(b) => !b,
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        _ => false,
    })
}

fn sequence_value(value: &Value) -> R<Option<PhpArray>> {
    if let Value::Object(o) = value {
        if o.class() == "ArrayObject"
            && let Some(Ok(Value::Array(a))) = o.call_method("getArrayCopy", &[])
        {
            return Ok(Some(a));
        }
        if o.instance_of("Traversable") {
            return Ok(Some(iterator_to_array(value, true)?));
        }
        return Ok(None);
    }
    Ok(value.as_array().cloned())
}

// ---------------------------------------------------------------------
// tests

/// The `empty` test.
pub fn test_empty(_: &Call<'_>, args: Vec<Value>) -> R {
    Ok(Value::Bool(test_empty_value(&arg(&args, 0))?))
}

/// The `iterable` test (`is_iterable`).
pub fn is_iterable(_: &Call<'_>, args: Vec<Value>) -> R {
    Ok(Value::Bool(is_iterable_value(&arg(&args, 0))))
}

/// The `sequence` test.
pub fn test_sequence(_: &Call<'_>, args: Vec<Value>) -> R {
    Ok(Value::Bool(
        sequence_value(&arg(&args, 0))?.is_some_and(|a| a.is_list()),
    ))
}

/// The `mapping` test.
pub fn test_mapping(_: &Call<'_>, args: Vec<Value>) -> R {
    let mut value = arg(&args, 0);
    if let Value::Object(o) = &value {
        if let Some(a) = o.downcast_ref::<php_value::object::ArrayObject>() {
            value = Value::Array(a.storage().clone());
        } else if o.instance_of("Traversable") {
            value = to_array(&value, true)?;
        }
    }
    Ok(Value::Bool(match &value {
        Value::Array(a) => !a.is_list(),
        Value::Object(_) => true,
        _ => false,
    }))
}

// ---------------------------------------------------------------------
// filters

/// The `default` filter.
pub fn default(_: &Call<'_>, args: Vec<Value>) -> R {
    let value = arg(&args, 0);
    if test_empty_value(&value)? {
        return Ok(arg_or(&args, 1, Value::String(PhpString::default())));
    }
    Ok(value)
}

/// The `abs` filter (`abs()`).
pub fn abs(_: &Call<'_>, args: Vec<Value>) -> R {
    let value = arg(&args, 0);
    let number = match &value {
        Value::Int(_) | Value::Float(_) => value.clone(),
        Value::Bool(_) | Value::Null => Value::Int(ops::to_int(&value)),
        Value::String(s) if php_value::numeric::is_numeric(s.as_bytes()) => ops::pos(&value)?,
        other => {
            return Err(type_error(format!(
                "abs(): Argument #1 ($num) must be of type int|float, {} given",
                other.debug_type()
            )));
        }
    };
    Ok(match number {
        Value::Int(i) if i == i64::MIN =>
        {
            #[allow(clippy::cast_precision_loss)]
            Value::Float(-(i as f64))
        }
        Value::Int(i) => Value::Int(i.abs()),
        Value::Float(f) => Value::Float(f.abs()),
        other => other,
    })
}

/// PHP's `round($value, $places)` (round half away from zero on the
/// shortest decimal form of the float).
pub fn php_round(value: f64, places: i64) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let places = places.clamp(-308, 308);
    let repr = format!("{:e}", value.abs());
    let (mantissa, exponent) = repr.split_once('e').unwrap_or((&repr, "0"));
    let exponent: i64 = exponent.parse().unwrap_or(0);
    let digits: Vec<u8> = mantissa
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|d| d - b'0')
        .collect();
    // value = 0.d1d2d3... * 10^(exponent + 1)
    let point = exponent + 1;
    let keep = point + places;
    if keep < 0 {
        return 0.0_f64.copysign(value);
    }
    let keep = usize::try_from(keep).unwrap_or(usize::MAX);
    if keep >= digits.len() {
        return value;
    }
    let mut kept: Vec<u8> = digits[..keep].to_vec();
    let mut carry = digits[keep] >= 5;
    let mut i = kept.len();
    while carry && i > 0 {
        i -= 1;
        if kept[i] == 9 {
            kept[i] = 0;
        } else {
            kept[i] += 1;
            carry = false;
        }
    }
    let mut point = point;
    if carry {
        kept.insert(0, 1);
        point += 1;
    }
    let mut text = String::from("0.");
    if kept.is_empty() {
        text.push('0');
    }
    for d in &kept {
        text.push(char::from(b'0' + d));
    }
    let _ = write!(text, "e{point}");
    let rounded: f64 = text.parse().unwrap_or(value.abs());
    rounded.copysign(value)
}

/// The `round` filter.
pub fn round(_: &Call<'_>, args: Vec<Value>) -> R {
    let value = ops::to_float(&arg(&args, 0));
    let precision = arg_or(&args, 1, Value::Int(0));
    let method = string_of(&arg_or(&args, 2, Value::from("common")))?;
    if method == b"common" {
        return Ok(Value::Float(php_round(value, ops::to_int(&precision))));
    }
    if method != b"ceil" && method != b"floor" {
        return Err(runtime(
            "The \"round\" filter only supports the \"common\", \"ceil\", and \"floor\" methods.",
        ));
    }
    let factor = ops::pow(&Value::Int(10), &precision)?;
    let scaled = ops::mul(&Value::Float(value), &factor)?;
    let f = ops::to_float(&scaled);
    let r = if method == b"ceil" {
        f.ceil()
    } else {
        f.floor()
    };
    ops::div(&Value::Float(r), &factor).map_err(Into::into)
}

/// PHP's `number_format()`.
#[must_use]
pub fn number_format(number: f64, decimals: i64, point: &[u8], separator: &[u8]) -> Vec<u8> {
    let decimals = decimals.max(0);
    let rounded = php_round(number, decimals);
    let places = usize::try_from(decimals).unwrap_or(0);
    let formatted = format!("{:.places$}", rounded.abs());
    let (int_part, frac_part) = formatted.split_once('.').unwrap_or((&formatted, ""));
    let mut out = Vec::new();
    let is_zero = rounded == 0.0;
    if rounded < 0.0 && !is_zero {
        out.push(b'-');
    }
    let int_bytes = int_part.as_bytes();
    for (i, d) in int_bytes.iter().enumerate() {
        if i > 0 && (int_bytes.len() - i) % 3 == 0 {
            out.extend_from_slice(separator);
        }
        out.push(*d);
    }
    if places > 0 {
        out.extend_from_slice(point);
        out.extend_from_slice(frac_part.as_bytes());
    }
    out
}

/// The `number_format` filter (`formatNumber()`).
pub fn format_number(call: &Call<'_>, args: Vec<Value>) -> R {
    let defaults = call.env.core_options().number_format.clone();
    let decimals = match arg(&args, 1) {
        Value::Null => defaults.0,
        v => ops::to_int(&v),
    };
    let point = match arg(&args, 2) {
        Value::Null => defaults.1,
        v => string_of(&v)?,
    };
    let separator = match arg(&args, 3) {
        Value::Null => defaults.2,
        v => string_of(&v)?,
    };
    let number = ops::to_float(&arg(&args, 0));
    Ok(Value::String(PhpString::from(number_format(
        number, decimals, &point, &separator,
    ))))
}

/// A date error as the exception PHP throws.
fn date_error(e: &php_date::DateError) -> Error {
    Error::exception(e.class(), e.message().as_bytes().to_vec())
}

/// `getTimezone()`: the zone set with `setTimezone()`, else PHP's
/// default zone.
fn extension_timezone(call: &Call<'_>) -> R<php_date::DateTimeZone> {
    match &call.env.core_options().timezone {
        Some(name) => php_date::DateTimeZone::new(name).map_err(|e| date_error(&e)),
        None => Ok(php_date::date_default_timezone_get()),
    }
}

/// The `$timezone` argument of `convertDate()`: `None` for `false` (leave
/// the zone as it is), the extension's zone for null, a `DateTimeZone`
/// object, or `new \DateTimeZone($timezone)`.
fn timezone_argument(call: &Call<'_>, timezone: &Value) -> R<Option<php_date::DateTimeZone>> {
    match timezone {
        Value::Bool(false) => Ok(None),
        Value::Null => extension_timezone(call).map(Some),
        other => match php_date::object::zone_of(other) {
            Some(zone) => Ok(Some(zone)),
            None => php_date::DateTimeZone::new(&text(&string_of(other)?))
                .map(Some)
                .map_err(|e| date_error(&e)),
        },
    }
}

/// `new \DateTime($text, $zone)`.
fn new_datetime(
    text: &str,
    zone: Option<&php_date::DateTimeZone>,
) -> R<php_date::DateTimeImmutable> {
    php_date::DateTimeImmutable::new(text, zone).map_err(|e| date_error(&e))
}

/// `convertDate()`, as a date and whether PHP gives a `DateTime` (a new
/// object, or for `modify()` the clone to change) or the
/// `DateTimeImmutable` itself.
fn converted(call: &Call<'_>, date: &Value, timezone: &Value) -> R<Value> {
    use php_date::object::DateTime;
    let zone = timezone_argument(call, timezone)?;
    if let Value::Object(o) = date {
        if let Some(immutable) = o.downcast_ref::<php_date::DateTimeImmutable>() {
            return Ok(match &zone {
                Some(zone) => Value::Object(Arc::new(immutable.set_timezone(zone))),
                None => date.clone(),
            });
        }
        if let Some(mutable) = o.downcast_ref::<DateTime>() {
            let copy = mutable.deep_clone();
            if let Some(zone) = &zone {
                copy.set_timezone(zone);
            }
            return Ok(Value::Object(Arc::new(copy)));
        }
    }
    let now = match date {
        Value::Null => true,
        Value::String(s) => s.as_bytes() == b"now",
        _ => false,
    };
    if now {
        let zone = match zone {
            Some(zone) => zone,
            None => extension_timezone(call)?,
        };
        return Ok(DateTime::object(new_datetime("now", Some(&zone))?));
    }
    let as_string = text(&string_of(date)?);
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let timestamp = digits(&as_string) || as_string.strip_prefix('-').is_some_and(digits);
    let parsed = if timestamp {
        new_datetime(&format!("@{as_string}"), None)?
    } else {
        new_datetime(&as_string, None)?
    };
    let parsed = match &zone {
        Some(zone) => parsed.set_timezone(zone),
        None => parsed,
    };
    Ok(DateTime::object(parsed))
}

/// The `date` function (`convertDate($date = null, $timezone = null)`):
/// a `DateTime` or `DateTimeImmutable` for a date, a timestamp, a date
/// text or null (now), in `$timezone` (null: the extension's zone; false:
/// the date's own).
pub fn convert_date(call: &Call<'_>, args: Vec<Value>) -> R {
    converted(call, &arg(&args, 0), &arg(&args, 1))
}

/// The `date` filter (`formatDate($date, $format = null, $timezone =
/// null)`). A `DateInterval` is formatted as it is (no zone).
pub fn format_date(call: &Call<'_>, args: Vec<Value>) -> R {
    let date = arg(&args, 0);
    let interval = match &date {
        Value::Object(o) => o.downcast_ref::<php_value::date_interval::DateInterval>(),
        _ => None,
    };
    let format = match arg(&args, 1) {
        Value::Null => {
            call.env.core_options().date_formats[usize::from(interval.is_some())].clone()
        }
        v => string_of(&v)?,
    };
    let format = text(&format);
    if let Some(interval) = interval {
        return Ok(Value::String(PhpString::from(interval.format(&format))));
    }
    let converted = converted(call, &date, &arg(&args, 2))?;
    let formatted = php_date::object::date_of(&converted)
        .map(|d| d.format(&format))
        .unwrap_or_default();
    Ok(Value::String(PhpString::from(formatted)))
}

/// The `date_modify` filter (`modifyDate($date, $modifier)`): the date
/// converted without a zone change, then `modify()`: a new
/// `DateTimeImmutable`, or the converted `DateTime` changed in place.
pub fn modify_date(call: &Call<'_>, args: Vec<Value>) -> R {
    let converted = converted(call, &arg(&args, 0), &Value::Bool(false))?;
    let modifier = text(&string_of(&arg(&args, 1))?);
    let Value::Object(o) = &converted else {
        return Ok(converted);
    };
    if let Some(mutable) = o.downcast_ref::<php_date::object::DateTime>() {
        mutable.modify(&modifier).map_err(|e| date_error(&e))?;
        return Ok(converted);
    }
    match o.call_method("modify", &[Value::String(PhpString::from(modifier))]) {
        Some(result) => {
            result.map_err(|t| Error::exception(t.class, t.message.as_bytes().to_vec()))
        }
        None => Ok(converted),
    }
}

/// The `url_encode` filter.
pub fn urlencode(_: &Call<'_>, args: Vec<Value>) -> R {
    let url = arg(&args, 0);
    if let Value::Array(a) = &url {
        let query =
            php_value::url::http_build_query_with(a, b"&", php_value::url::QueryEncoding::Rfc3986)
                .unwrap_or_default();
        return Ok(Value::String(PhpString::from(query)));
    }
    Ok(Value::String(PhpString::from(
        php_value::url::rawurlencode(&string_or_empty(&url)?),
    )))
}

/// The `json_encode` filter (`json_encode()`).
pub fn json_encode(_: &Call<'_>, args: Vec<Value>) -> R {
    let flags = ops::to_int(&arg_or(&args, 1, Value::Int(0)));
    let depth = usize::try_from(ops::to_int(&arg_or(&args, 2, Value::Int(512)))).unwrap_or(512);
    match php_value::json::encode(&arg(&args, 0), flags, depth) {
        Ok(encoded) => Ok(encoded
            .json
            .map_or(Value::Bool(false), |j| Value::String(PhpString::from(j)))),
        Err(php_value::json::EncodeFailure::Thrown(t)) => Err(t.into()),
        Err(php_value::json::EncodeFailure::Json { error, .. }) => Err(Error::exception(
            "JsonException",
            format!("{error}").into_bytes(),
        )),
    }
}

/// The `title` filter (`titleCase()`).
pub fn title_case(call: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    Ok(Value::String(PhpString::from(mb::mb_title_case(
        &s,
        call.charset(),
    ))))
}

/// The `capitalize` filter.
pub fn capitalize(call: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    let charset = call.charset();
    let mut out = mb::mb_strtoupper(&mb::mb_substr(&s, 0, Some(1), charset), charset);
    out.extend(mb::mb_strtolower(
        &mb::mb_substr(&s, 1, None, charset),
        charset,
    ));
    Ok(Value::String(PhpString::from(out)))
}

/// The `upper` filter.
pub fn upper(call: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    Ok(Value::String(PhpString::from(mb::mb_strtoupper(
        &s,
        call.charset(),
    ))))
}

/// The `lower` filter.
pub fn lower(call: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    Ok(Value::String(PhpString::from(mb::mb_strtolower(
        &s,
        call.charset(),
    ))))
}

/// PHP's `strip_tags($string, $allowed)`: removes tags, comments and PHP
/// code; the allowed tags (`<a><b>` or a list of names) are kept.
pub fn strip_tags(s: &[u8], allowed: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c != b'<' {
            out.push(c);
            i += 1;
            continue;
        }
        let next = s.get(i + 1).copied();
        // a "<" followed by a space or the end is text
        if next.is_none_or(|n| n.is_ascii_whitespace()) {
            out.push(c);
            i += 1;
            continue;
        }
        // a comment
        if s[i..].starts_with(b"<!--") {
            match find(&s[i + 4..], b"-->") {
                Some(end) => i += 4 + end + 3,
                None => i = s.len(),
            }
            continue;
        }
        // a tag: up to the matching '>' outside quotes
        let mut j = i + 1;
        let mut quote: Option<u8> = None;
        let mut depth = 1;
        while j < s.len() {
            let d = s[j];
            match quote {
                Some(q) if d == q => quote = None,
                Some(_) => {}
                None => match d {
                    b'"' | b'\'' => quote = Some(d),
                    b'<' => depth += 1,
                    b'>' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                },
            }
            j += 1;
        }
        let tag = &s[i..j.min(s.len())];
        if !allowed.is_empty() {
            let name: Vec<u8> = tag[1..]
                .iter()
                .skip_while(|b| **b == b'/')
                .take_while(|b| b.is_ascii_alphanumeric())
                .map(u8::to_ascii_lowercase)
                .collect();
            if allowed.contains(&name) {
                out.extend_from_slice(tag);
                if j < s.len() {
                    out.push(b'>');
                }
            }
        }
        i = j + 1;
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The `striptags` filter.
pub fn striptags(_: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    let allowed: Vec<Vec<u8>> = match arg(&args, 1) {
        Value::Null => Vec::new(),
        Value::Array(a) => a
            .values()
            .map(|v| string_of(v).map(|b| b.to_ascii_lowercase()))
            .collect::<R<_>>()?,
        v => {
            let list = string_of(&v)?.to_ascii_lowercase();
            list.split(|b| *b == b'<' || *b == b'>')
                .filter(|p| !p.is_empty())
                .map(<[u8]>::to_vec)
                .collect()
        }
    };
    Ok(Value::String(PhpString::from(strip_tags(&s, &allowed))))
}

/// The `trim` filter.
pub fn trim(call: &Call<'_>, args: Vec<Value>) -> R {
    let value = arg(&args, 0);
    let mask_value = arg(&args, 1);
    let mask = if mask_value.is_null() {
        DEFAULT_TRIM_CHARS.to_vec()
    } else {
        string_of(&mask_value)?
    };
    let side = string_of(&arg_or(&args, 2, Value::from("both")))?;
    let s = string_or_empty(&value)?;
    let mask = charmask(&mask);
    let trimmed = match side.as_slice() {
        b"both" => trim_bytes(&s, &mask, true, true),
        b"left" => trim_bytes(&s, &mask, true, false),
        b"right" => trim_bytes(&s, &mask, false, true),
        _ => {
            return Err(runtime(
                "Trimming side must be \"left\", \"right\" or \"both\".",
            ));
        }
    };
    let _ = call;
    if let Value::Object(o) = &value
        && let Some(markup) = o.downcast_ref::<Markup>()
        && mask_value.is_null()
    {
        return Ok(Markup::value(trimmed, markup.charset().clone()));
    }
    Ok(Value::String(PhpString::from(trimmed)))
}

/// PHP's character mask (`a..z` ranges).
fn charmask(chars: &[u8]) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut i = 0;
    while i < chars.len() {
        if i + 3 < chars.len()
            && chars.get(i + 1) == Some(&b'.')
            && chars.get(i + 2) == Some(&b'.')
            && i + 3 < chars.len()
            && chars[i + 3] >= chars[i]
        {
            for c in chars[i]..=chars[i + 3] {
                mask[usize::from(c)] = true;
            }
            i += 4;
        } else {
            mask[usize::from(chars[i])] = true;
            i += 1;
        }
    }
    mask
}

fn trim_bytes(s: &[u8], mask: &[bool; 256], left: bool, right: bool) -> Vec<u8> {
    let mut start = 0;
    let mut end = s.len();
    if left {
        while start < end && mask[usize::from(s[start])] {
            start += 1;
        }
    }
    if right {
        while end > start && mask[usize::from(s[end - 1])] {
            end -= 1;
        }
    }
    s[start..end].to_vec()
}

/// The `nl2br` filter.
pub fn nl2br(_: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        match (s[i], s.get(i + 1)) {
            (b'\r', Some(b'\n')) | (b'\n', Some(b'\r')) => {
                out.extend_from_slice(b"<br />");
                out.push(s[i]);
                out.push(s[i + 1]);
                i += 2;
            }
            (b'\r' | b'\n', _) => {
                out.extend_from_slice(b"<br />");
                out.push(s[i]);
                i += 1;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(Value::String(PhpString::from(out)))
}

/// The `spaceless` filter.
pub fn spaceless(_: &Call<'_>, args: Vec<Value>) -> R {
    let s = string_or_empty(&arg(&args, 0))?;
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'>' {
            let mut j = i + 1;
            while j < s.len() && matches!(s[j], b' ' | b'\t' | b'\n' | b'\r' | b'\x0B' | b'\x0C') {
                j += 1;
            }
            if j > i + 1 && j < s.len() && s[j] == b'<' {
                out.push(b'>');
                i = j;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    let mask = charmask(DEFAULT_TRIM_CHARS);
    Ok(Value::String(PhpString::from(trim_bytes(
        &out, &mask, true, true,
    ))))
}

/// The `join` filter.
pub fn join(_: &Call<'_>, args: Vec<Value>) -> R {
    let mut value = arg(&args, 0);
    if !is_iterable_value(&value) {
        value = Value::Array(php_value::object::to_array(&value));
    }
    let values: Vec<Value> = array_of(&value, false)?.values().cloned().collect();
    let glue = string_of(&arg_or(&args, 1, Value::from("")))?;
    let and = arg(&args, 2);
    if values.is_empty() {
        return Ok(Value::from(""));
    }
    let parts: Vec<Vec<u8>> = values.iter().map(string_of).collect::<R<_>>()?;
    let and_bytes = if and.is_null() {
        None
    } else {
        Some(string_of(&and)?)
    };
    match and_bytes {
        Some(and) if and != glue => {
            if parts.len() == 1 {
                return Ok(values[0].clone());
            }
            let mut out = parts[..parts.len() - 1].join(glue.as_slice());
            out.extend_from_slice(&and);
            out.extend_from_slice(&parts[parts.len() - 1]);
            Ok(Value::String(PhpString::from(out)))
        }
        _ => Ok(Value::String(PhpString::from(parts.join(glue.as_slice())))),
    }
}

/// PHP's `explode($delimiter, $string, $limit)`.
fn explode(delimiter: &[u8], s: &[u8], limit: Option<i64>) -> Vec<Vec<u8>> {
    let mut parts: Vec<Vec<u8>> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + delimiter.len() <= s.len() {
        if &s[i..i + delimiter.len()] == delimiter {
            parts.push(s[start..i].to_vec());
            i += delimiter.len();
            start = i;
        } else {
            i += 1;
        }
    }
    parts.push(s[start..].to_vec());
    match limit {
        Some(l) if l > 0 => {
            let l = usize::try_from(l).unwrap_or(usize::MAX);
            if parts.len() > l {
                let tail = parts[l - 1..].join(delimiter);
                parts.truncate(l - 1);
                parts.push(tail);
            }
            parts
        }
        Some(l) if l < 0 => {
            let drop = usize::try_from(-l).unwrap_or(usize::MAX);
            if drop >= parts.len() {
                Vec::new()
            } else {
                parts.truncate(parts.len() - drop);
                parts
            }
        }
        Some(_) => vec![parts.join(delimiter)],
        None => parts,
    }
}

/// The `split` filter.
pub fn split(call: &Call<'_>, args: Vec<Value>) -> R {
    let value = string_or_empty(&arg(&args, 0))?;
    let delimiter = string_of(&arg(&args, 1))?;
    let limit = arg(&args, 2);
    let limit = if limit.is_null() {
        None
    } else {
        Some(ops::to_int(&limit))
    };
    let list = |parts: Vec<Vec<u8>>| {
        Value::Array(PhpArray::list(
            parts.into_iter().map(|p| Value::String(PhpString::from(p))),
        ))
    };
    if !delimiter.is_empty() {
        return Ok(list(explode(&delimiter, &value, limit)));
    }
    let limit = limit.unwrap_or(0);
    if limit <= 1 {
        return Ok(list(mb::split_into_characters(&value, "split")?));
    }
    let charset = call.charset();
    let length = mb::mb_strlen(&value, charset);
    if length < limit {
        return Ok(list(vec![value]));
    }
    let mut parts = Vec::new();
    let mut i = 0;
    while i < length {
        parts.push(mb::mb_substr(&value, i, Some(limit), charset));
        i += limit;
    }
    Ok(list(parts))
}

/// The `merge` filter.
pub fn merge(_: &Call<'_>, args: Vec<Value>) -> R {
    Ok(Value::Array(merge_arrays(&args)?))
}

/// `CoreExtension::merge(...$arrays)`.
pub fn merge_arrays(arrays: &[Value]) -> R<PhpArray> {
    let mut result = PhpArray::new();
    for (n, array) in arrays.iter().enumerate() {
        if !is_iterable_value(array) {
            return Err(runtime(format!(
                "The \"merge\" filter expects a sequence or a mapping, got \"{}\" for argument {}.",
                array.debug_type(),
                n + 1
            )));
        }
        for (key, value) in array_of(array, true)?.iter() {
            match key {
                Key::Int(_) => {
                    let _ = result.push(value.clone());
                }
                Key::Str(_) => result.insert(key.clone(), value.clone()),
            }
        }
    }
    Ok(result)
}

/// The `replace` filter.
pub fn replace(_: &Call<'_>, args: Vec<Value>) -> R {
    let from = arg(&args, 1);
    if !is_iterable_value(&from) {
        return Err(runtime(format!(
            "The \"replace\" filter expects a sequence or a mapping, got \"{}\".",
            from.debug_type()
        )));
    }
    let s = string_or_empty(&arg(&args, 0))?;
    let pairs: Vec<(Vec<u8>, Vec<u8>)> = array_of(&from, true)?
        .iter()
        .map(|(k, v)| Ok((k.bytes().into_owned(), string_of(v)?)))
        .collect::<R<_>>()?;
    Ok(Value::String(PhpString::from(strtr(&s, &pairs))))
}

/// PHP's `strtr($string, $pairs)`: longest match first, no rescanning.
#[must_use]
pub fn strtr(s: &[u8], pairs: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let pairs: Vec<&(Vec<u8>, Vec<u8>)> = pairs.iter().filter(|(k, _)| !k.is_empty()).collect();
    if pairs.is_empty() {
        return s.to_vec();
    }
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let best = pairs
            .iter()
            .filter(|(k, _)| s[i..].starts_with(k))
            .max_by_key(|(k, _)| k.len());
        if let Some((k, v)) = best {
            out.extend_from_slice(v);
            i += k.len();
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// `array_slice($array, $offset, $length, $preserveKeys)`.
fn array_slice(
    array: &PhpArray,
    offset: i64,
    length: Option<i64>,
    preserve_keys: bool,
) -> PhpArray {
    let len = count_i64(array.len());
    let mut start = if offset < 0 {
        (len + offset).max(0)
    } else {
        offset.min(len)
    };
    let end = match length {
        None => len,
        Some(l) if l < 0 => (len + l).max(0),
        Some(l) => start.saturating_add(l).min(len),
    };
    if end < start {
        start = end;
    }
    let mut out = PhpArray::new();
    for (i, (k, v)) in array.iter().enumerate() {
        let i = count_i64(i);
        if i < start || i >= end {
            continue;
        }
        match k {
            Key::Int(_) if !preserve_keys => {
                let _ = out.push(v.clone());
            }
            _ => out.insert(k.clone(), v.clone()),
        }
    }
    out
}

/// `slice($charset, $item, $start, $length, $preserveKeys)`.
pub fn slice_value(
    charset: &[u8],
    item: &Value,
    start: i64,
    length: Option<i64>,
    preserve_keys: bool,
) -> R {
    let mut item = item.clone();
    if is_traversable(&item) {
        if start >= 0 && length.is_none_or(|l| l >= 0) {
            let pairs = traverse(&item)?;
            let from = usize::try_from(start).unwrap_or(usize::MAX);
            let mut out = PhpArray::new();
            let take = length.map_or(usize::MAX, |l| usize::try_from(l).unwrap_or(0));
            for (k, v) in pairs.into_iter().skip(from).take(take) {
                if preserve_keys {
                    out.insert(key_of(&k)?, v);
                } else {
                    let _ = out.push(v);
                }
            }
            return Ok(Value::Array(out));
        }
        item = Value::Array(iterator_to_array(&item, preserve_keys)?);
    }
    if let Value::Array(a) = &item {
        return Ok(Value::Array(array_slice(a, start, length, preserve_keys)));
    }
    let s = string_of(&item)?;
    Ok(Value::String(PhpString::from(mb::mb_substr(
        &s, start, length, charset,
    ))))
}

/// The `slice` filter.
pub fn slice(call: &Call<'_>, args: Vec<Value>) -> R {
    let length = arg(&args, 2);
    slice_value(
        call.charset(),
        &arg(&args, 0),
        ops::to_int(&arg(&args, 1)),
        if length.is_null() {
            None
        } else {
            Some(ops::to_int(&length))
        },
        arg(&args, 3).is_truthy(),
    )
}

fn first_of(elements: Value) -> Value {
    match elements {
        Value::String(_) => elements,
        Value::Array(a) => a.first().map_or(Value::Bool(false), |(_, v)| v.clone()),
        other => other,
    }
}

/// The `first` filter.
pub fn first(call: &Call<'_>, args: Vec<Value>) -> R {
    Ok(first_of(slice_value(
        call.charset(),
        &arg(&args, 0),
        0,
        Some(1),
        false,
    )?))
}

/// The `last` filter.
pub fn last(call: &Call<'_>, args: Vec<Value>) -> R {
    Ok(first_of(slice_value(
        call.charset(),
        &arg(&args, 0),
        -1,
        Some(1),
        false,
    )?))
}

/// The `keys` filter.
pub fn keys(_: &Call<'_>, args: Vec<Value>) -> R {
    let value = arg(&args, 0);
    if is_traversable(&value) {
        return Ok(Value::Array(PhpArray::list(
            traverse(&value)?.into_iter().map(|(k, _)| k),
        )));
    }
    match value {
        Value::Array(a) => Ok(Value::Array(PhpArray::list(a.keys().map(Key::to_value)))),
        _ => Ok(Value::Array(PhpArray::new())),
    }
}

/// The `invoke` filter.
pub fn invoke(call: &Call<'_>, args: Vec<Value>) -> R {
    let mut args = args.into_iter();
    let arrow = args.next().unwrap_or(Value::Null);
    let rest: Vec<Value> = args.collect();
    super::call_arrow(call.env, &arrow, &rest)
}

fn reverse_array(array: &PhpArray, preserve_keys: bool) -> PhpArray {
    let mut out = PhpArray::new();
    let items: Vec<(&Key, &Value)> = array.iter().collect();
    for (k, v) in items.into_iter().rev() {
        match k {
            Key::Int(_) if !preserve_keys => {
                let _ = out.push(v.clone());
            }
            _ => out.insert(k.clone(), v.clone()),
        }
    }
    out
}

/// The `reverse` filter.
pub fn reverse(_: &Call<'_>, args: Vec<Value>) -> R {
    let item = arg(&args, 0);
    let preserve_keys = arg(&args, 1).is_truthy();
    if is_traversable(&item) {
        return Ok(Value::Array(reverse_array(
            &iterator_to_array(&item, true)?,
            preserve_keys,
        )));
    }
    if let Value::Array(a) = &item {
        return Ok(Value::Array(reverse_array(a, preserve_keys)));
    }
    let s = string_of(&item)?;
    let mut chars = mb::split_into_characters(&s, "reverse")?;
    chars.reverse();
    Ok(Value::String(PhpString::from(chars.concat())))
}

fn random_index(len: usize) -> usize {
    use std::hash::{BuildHasher, Hasher};
    if len == 0 {
        return 0;
    }
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_usize(len);
    usize::try_from(hasher.finish() % (len as u64)).unwrap_or(0)
}

fn shuffle_vec<T>(items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = random_index(i + 1);
        items.swap(i, j);
    }
}

/// The `shuffle` filter.
pub fn shuffle(_: &Call<'_>, args: Vec<Value>) -> R {
    let item = arg(&args, 0);
    if let Value::String(s) = &item {
        let mut chars = mb::split_into_characters(s.as_bytes(), "shuffle")?;
        shuffle_vec(&mut chars);
        return Ok(Value::String(PhpString::from(chars.concat())));
    }
    if is_iterable_value(&item) {
        let mut values: Vec<Value> = array_of(&item, false)?.values().cloned().collect();
        shuffle_vec(&mut values);
        return Ok(Value::Array(PhpArray::list(values)));
    }
    Ok(item)
}

/// The `length` filter.
pub fn length(call: &Call<'_>, args: Vec<Value>) -> R {
    let thing = arg(&args, 0);
    let charset = call.charset();
    Ok(Value::Int(match &thing {
        Value::Null => 0,
        Value::Array(a) => count_i64(a.len()),
        Value::Object(o) => {
            if let Some(count) = o.php_count() {
                count?
            } else if o.instance_of("Traversable") {
                count_i64(traverse(&thing)?.len())
            } else if o.is_stringable() {
                mb::mb_strlen(&string_of(&thing)?, charset)
            } else {
                1
            }
        }
        scalar => mb::mb_strlen(&string_of(scalar)?, charset),
    }))
}

/// PHP's `asort()`/`uasort()` comparison result of an arrow.
fn sort_pairs(env: &Environment, array: &PhpArray, arrow: Option<&Value>) -> R<PhpArray> {
    let mut items: Vec<(Key, Value)> = array.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let mut error: Option<Error> = None;
    items.sort_by(|(_, a), (_, b)| {
        if error.is_some() {
            return Ordering::Equal;
        }
        match arrow {
            None => ops::compare(a, b),
            Some(arrow) => match super::call_arrow(env, arrow, &[a.clone(), b.clone()]) {
                Ok(r) => ops::to_int(&r).cmp(&0),
                Err(e) => {
                    error = Some(e);
                    Ordering::Equal
                }
            },
        }
    });
    if let Some(e) = error {
        return Err(e);
    }
    let mut out = PhpArray::new();
    for (k, v) in items {
        out.insert(k, v);
    }
    Ok(out)
}

/// The `sort` filter.
pub fn sort(call: &Call<'_>, args: Vec<Value>) -> R {
    let array = arg(&args, 0);
    let array = if is_traversable(&array) {
        iterator_to_array(&array, true)?
    } else if let Value::Array(a) = array {
        a
    } else {
        return Err(runtime(format!(
            "The \"sort\" filter expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    };
    let arrow = arg(&args, 1);
    let arrow = if arrow.is_null() { None } else { Some(&arrow) };
    Ok(Value::Array(sort_pairs(call.env, &array, arrow)?))
}

/// The `batch` filter.
pub fn batch(_: &Call<'_>, args: Vec<Value>) -> R {
    let items = arg(&args, 0);
    if !is_iterable_value(&items) {
        return Err(runtime(format!(
            "The \"batch\" filter expects a sequence or a mapping, got \"{}\".",
            items.debug_type()
        )));
    }
    let size = ops::to_float(&arg(&args, 1)).ceil();
    let fill = arg(&args, 2);
    let preserve_keys = arg_or(&args, 3, Value::Bool(true)).is_truthy();
    let size = ops::float_to_int(size);
    if size < 1 {
        return Err(value_error(
            "array_chunk(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let size_usize = usize::try_from(size).unwrap_or(usize::MAX);
    let array = array_of(&items, preserve_keys)?;
    let mut result: Vec<PhpArray> = Vec::new();
    for (k, v) in array.iter() {
        if result.last().is_none_or(|c| c.len() >= size_usize) {
            result.push(PhpArray::new());
        }
        if let Some(chunk) = result.last_mut() {
            if preserve_keys {
                chunk.insert(k.clone(), v.clone());
            } else {
                let _ = chunk.push(v.clone());
            }
        }
    }
    if !fill.is_null()
        && let Some(last) = result.last_mut()
    {
        let missing = size_usize.saturating_sub(last.len());
        for _ in 0..missing {
            let _ = last.push(fill.clone());
        }
    }
    Ok(Value::Array(PhpArray::list(
        result.into_iter().map(Value::Array),
    )))
}

/// The `column` filter (`array_column()`).
pub fn column(_: &Call<'_>, args: Vec<Value>) -> R {
    let array = arg(&args, 0);
    if !is_iterable_value(&array) {
        return Err(runtime(format!(
            "The \"column\" filter expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    }
    let array = array_of(&array, true)?;
    let name = arg(&args, 1);
    let index = arg(&args, 2);
    let read = |row: &Value, key: &Value| -> Option<Value> {
        match row {
            Value::Array(a) => Key::from_value(key).and_then(|k| a.get(&k).cloned()),
            Value::Object(o) => {
                let k = string_of(key).ok()?;
                o.property(&k)
            }
            _ => None,
        }
    };
    let mut out = PhpArray::new();
    for row in array.values() {
        let value = if name.is_null() {
            Some(row.clone())
        } else {
            read(row, &name)
        };
        let Some(value) = value else {
            continue;
        };
        let key = if index.is_null() {
            None
        } else {
            read(row, &index)
        };
        match key.and_then(|k| Key::from_value(&k)) {
            Some(k) => out.insert(k, value),
            None => {
                let _ = out.push(value);
            }
        }
    }
    Ok(Value::Array(out))
}

fn check_arrow(env: &Environment, arrow: &Value, thing: &str, kind: &str) {
    let _ = env;
    let is_closure = matches!(arrow, Value::Object(o) if o.class() == "Closure");
    if !is_closure {
        symfony_deprecation_contracts::trigger_deprecation(
            b"twig/twig",
            b"3.15",
            format!("Passing a callable that is not a PHP \\Closure as an argument to the \"{thing}\" {kind} is deprecated.").as_bytes(),
        );
    }
}

/// The `filter` filter.
pub fn filter(call: &Call<'_>, args: Vec<Value>) -> R {
    let array = arg(&args, 0);
    let arrow = arg(&args, 1);
    if !is_iterable_value(&array) {
        return Err(runtime(format!(
            "The \"filter\" filter expects a sequence/mapping or \"Traversable\", got \"{}\".",
            array.debug_type()
        )));
    }
    check_arrow(call.env, &arrow, "filter", "filter");
    let mut out = PhpArray::new();
    for (k, v) in traverse(&array)? {
        if super::call_arrow(call.env, &arrow, &[v.clone(), k.clone()])?.is_truthy() {
            out.insert(key_of(&k)?, v);
        }
    }
    if is_traversable(&array) {
        return Ok(Value::Object(Arc::new(
            php_value::object::ArrayIterator::new(out),
        )));
    }
    Ok(Value::Array(out))
}

/// The `find` filter.
pub fn find_filter(call: &Call<'_>, args: Vec<Value>) -> R {
    let array = arg(&args, 0);
    let arrow = arg(&args, 1);
    if !is_iterable_value(&array) {
        return Err(runtime(format!(
            "The \"find\" filter expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    }
    check_arrow(call.env, &arrow, "find", "filter");
    for (k, v) in traverse(&array)? {
        if super::call_arrow(call.env, &arrow, &[v.clone(), k])?.is_truthy() {
            return Ok(v);
        }
    }
    Ok(Value::Null)
}

/// The `map` filter.
pub fn map(call: &Call<'_>, args: Vec<Value>) -> R {
    let array = arg(&args, 0);
    let arrow = arg(&args, 1);
    if !is_iterable_value(&array) {
        return Err(runtime(format!(
            "The \"map\" filter expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    }
    check_arrow(call.env, &arrow, "map", "filter");
    let mut out = PhpArray::new();
    for (k, v) in traverse(&array)? {
        let mapped = super::call_arrow(call.env, &arrow, &[v, k.clone()])?;
        out.insert(key_of(&k)?, mapped);
    }
    Ok(Value::Array(out))
}

/// The `reduce` filter.
pub fn reduce(call: &Call<'_>, args: Vec<Value>) -> R {
    let array = arg(&args, 0);
    let arrow = arg(&args, 1);
    if !is_iterable_value(&array) {
        return Err(runtime(format!(
            "The \"reduce\" filter expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    }
    check_arrow(call.env, &arrow, "reduce", "filter");
    let mut accumulator = arg(&args, 2);
    for (k, v) in traverse(&array)? {
        accumulator = super::call_arrow(call.env, &arrow, &[accumulator, v, k])?;
    }
    Ok(accumulator)
}

/// The `has some` operator (`arraySome()`).
pub fn array_some(env: &Environment, array: &Value, arrow: &Value) -> R<bool> {
    if !is_iterable_value(array) {
        return Err(runtime(format!(
            "The \"has some\" test expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    }
    check_arrow(env, arrow, "has some", "operator");
    for (k, v) in traverse(array)? {
        if super::call_arrow(env, arrow, &[v, k])?.is_truthy() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The `has every` operator (`arrayEvery()`).
pub fn array_every(env: &Environment, array: &Value, arrow: &Value) -> R<bool> {
    if !is_iterable_value(array) {
        return Err(runtime(format!(
            "The \"has every\" test expects a sequence or a mapping, got \"{}\".",
            array.debug_type()
        )));
    }
    check_arrow(env, arrow, "has every", "operator");
    for (k, v) in traverse(array)? {
        if !super::call_arrow(env, arrow, &[v, k])?.is_truthy() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// PHP's `sprintf()`.
#[allow(clippy::too_many_lines)]
pub fn php_sprintf(format: &[u8], values: &[Value]) -> R<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut next_arg = 0usize;
    let too_few = |n: usize| {
        Error::exception(
            "ArgumentCountError",
            format!(
                "{} arguments are required, {} given",
                n + 1,
                values.len() + 1
            )
            .into_bytes(),
        )
    };
    while i < format.len() {
        if format[i] != b'%' {
            out.push(format[i]);
            i += 1;
            continue;
        }
        i += 1;
        if format.get(i) == Some(&b'%') {
            out.push(b'%');
            i += 1;
            continue;
        }
        // argnum
        let mut argnum: Option<usize> = None;
        let mut j = i;
        while j < format.len() && format[j].is_ascii_digit() {
            j += 1;
        }
        if j > i && format.get(j) == Some(&b'$') {
            let n: usize = std::str::from_utf8(&format[i..j])
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(0);
            if n == 0 {
                return Err(value_error(
                    "Argument number specifier must be greater than zero and less than 2147483647",
                ));
            }
            argnum = Some(n - 1);
            i = j + 1;
        }
        // flags
        let mut left = false;
        let mut plus = false;
        let mut pad = b' ';
        loop {
            match format.get(i) {
                Some(b'-') => left = true,
                Some(b'+') => plus = true,
                Some(b'0') => pad = b'0',
                Some(b' ') => pad = b' ',
                Some(b'\'') => {
                    i += 1;
                    if let Some(&c) = format.get(i) {
                        pad = c;
                    }
                }
                _ => break,
            }
            i += 1;
        }
        let mut width = 0usize;
        while let Some(d) = format.get(i).filter(|d| d.is_ascii_digit()) {
            width = width * 10 + usize::from(d - b'0');
            i += 1;
        }
        let mut precision: Option<usize> = None;
        if format.get(i) == Some(&b'.') {
            i += 1;
            let mut p = 0usize;
            while let Some(d) = format.get(i).filter(|d| d.is_ascii_digit()) {
                p = p * 10 + usize::from(d - b'0');
                i += 1;
            }
            precision = Some(p);
        }
        let Some(&spec) = format.get(i) else {
            return Err(value_error("Missing format specifier at end of string"));
        };
        i += 1;
        let index = if let Some(n) = argnum {
            n
        } else {
            let n = next_arg;
            next_arg += 1;
            n
        };
        let value = values.get(index).ok_or_else(|| too_few(index + 1))?;
        let mut body: Vec<u8> = match spec {
            b's' => {
                let s = string_of(value)?;
                match precision {
                    Some(p) if p < s.len() => s[..p].to_vec(),
                    _ => s,
                }
            }
            b'd' => {
                let n = ops::to_int(value);
                let mut s = n.to_string().into_bytes();
                if plus && n >= 0 {
                    s.insert(0, b'+');
                }
                s
            }
            b'u' => {
                #[allow(clippy::cast_sign_loss)]
                let n = ops::to_int(value) as u64;
                n.to_string().into_bytes()
            }
            b'f' | b'F' => {
                let f = ops::to_float(value);
                let p = precision.unwrap_or(6);
                let mut s =
                    format!("{:.p$}", php_round(f, i64::try_from(p).unwrap_or(6))).into_bytes();
                if plus && f >= 0.0 {
                    s.insert(0, b'+');
                }
                s
            }
            b'e' | b'E' => {
                let f = ops::to_float(value);
                let p = precision.unwrap_or(6);
                let s = format!("{f:.p$e}");
                let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
                let e: i64 = e.parse().unwrap_or(0);
                let mut t = format!("{m}e{}{}", if e < 0 { "-" } else { "+" }, e.abs());
                if spec == b'E' {
                    t = t.to_uppercase();
                }
                t.into_bytes()
            }
            b'x' => format!("{:x}", ops::to_int(value)).into_bytes(),
            b'X' => format!("{:X}", ops::to_int(value)).into_bytes(),
            b'o' => format!("{:o}", ops::to_int(value)).into_bytes(),
            b'b' => format!("{:b}", ops::to_int(value)).into_bytes(),
            b'c' => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let c = ops::to_int(value) as u8;
                vec![c]
            }
            other => {
                return Err(value_error(format!(
                    "Unknown format specifier \"{}\"",
                    char::from(other)
                )));
            }
        };
        if body.len() < width {
            let fill = width - body.len();
            if left {
                body.extend(std::iter::repeat_n(
                    if pad == b'0' { b' ' } else { pad },
                    fill,
                ));
            } else if pad == b'0' && matches!(body.first(), Some(b'-' | b'+')) && spec != b's' {
                let sign = body.remove(0);
                let mut padded = vec![sign];
                padded.extend(std::iter::repeat_n(b'0', fill));
                padded.extend(body);
                body = padded;
            } else {
                let mut padded: Vec<u8> = std::iter::repeat_n(pad, fill).collect();
                padded.extend(body);
                body = padded;
            }
        }
        out.extend(body);
    }
    Ok(out)
}

/// The `format` filter (`sprintf()`).
pub fn sprintf(_: &Call<'_>, args: Vec<Value>) -> R {
    let mut args = args.into_iter();
    let format = args.next().unwrap_or(Value::Null);
    let format = string_or_empty(&format)?;
    let values: Vec<Value> = args.collect();
    Ok(Value::String(PhpString::from(php_sprintf(
        &format, &values,
    )?)))
}

// ---------------------------------------------------------------------
// functions

fn max_min(args: &[Value], name: &str, want: Ordering) -> R {
    let values: Vec<Value> = if args.len() == 1 {
        match &args[0] {
            Value::Array(a) => {
                if a.is_empty() {
                    return Err(value_error(format!(
                        "{name}(): Argument #1 ($value) must contain at least one element"
                    )));
                }
                a.values().cloned().collect()
            }
            other => {
                return Err(type_error(format!(
                    "{name}(): Argument #1 ($value) must be of type array, {} given",
                    other.debug_type()
                )));
            }
        }
    } else {
        args.to_vec()
    };
    let mut best = values[0].clone();
    for v in &values[1..] {
        let better = match want {
            Ordering::Greater => ops::compare(&best, v) == Ordering::Less,
            _ => ops::compare(v, &best) == Ordering::Less,
        };
        if better {
            best = v.clone();
        }
    }
    Ok(best)
}

/// The `max` function (`max()`).
pub fn max(_: &Call<'_>, args: Vec<Value>) -> R {
    if args.is_empty() {
        return Err(Error::exception(
            "ArgumentCountError",
            "max() expects at least 1 argument, 0 given",
        ));
    }
    max_min(&args, "max", Ordering::Greater)
}

/// The `min` function (`min()`).
pub fn min(_: &Call<'_>, args: Vec<Value>) -> R {
    if args.is_empty() {
        return Err(Error::exception(
            "ArgumentCountError",
            "min() expects at least 1 argument, 0 given",
        ));
    }
    max_min(&args, "min", Ordering::Less)
}

/// PHP's `range($start, $end, $step)`.
pub fn range_values(start: &Value, end: &Value, step: &Value) -> R {
    let step_f = ops::to_float(step);
    let step_is_float = matches!(step, Value::Float(f) if f.fract() != 0.0);
    let char_range = |v: &Value| matches!(v, Value::String(s) if !s.is_empty() && !php_value::numeric::is_numeric(s.as_bytes()));
    if char_range(start) && char_range(end) && !step_is_float {
        let (Value::String(a), Value::String(b)) = (start, end) else {
            unreachable!();
        };
        let (a, b) = (a.as_bytes()[0], b.as_bytes()[0]);
        let step = ops::to_int(step).unsigned_abs();
        if step == 0 {
            return Err(value_error("range(): Argument #3 ($step) cannot be 0"));
        }
        let step = u8::try_from(step).unwrap_or(u8::MAX);
        let mut out = Vec::new();
        if a <= b {
            let mut c = a;
            loop {
                out.push(Value::String(PhpString::from(vec![c])));
                match c.checked_add(step) {
                    Some(n) if n <= b => c = n,
                    _ => break,
                }
            }
        } else {
            let mut c = a;
            loop {
                out.push(Value::String(PhpString::from(vec![c])));
                match c.checked_sub(step) {
                    Some(n) if n >= b => c = n,
                    _ => break,
                }
            }
        }
        return Ok(Value::Array(PhpArray::list(out)));
    }
    let number = |v: &Value| -> Value {
        match v {
            Value::String(s) => match php_value::numeric::numeric_value(s.as_bytes()) {
                Some(php_value::Number::Int(i)) => Value::Int(i),
                Some(php_value::Number::Float(f)) => Value::Float(f),
                None => Value::Int(0),
            },
            Value::Float(f) => Value::Float(*f),
            other => Value::Int(ops::to_int(other)),
        }
    };
    let (s, e) = (number(start), number(end));
    if step_f == 0.0 {
        return Err(value_error("range(): Argument #3 ($step) cannot be 0"));
    }
    let is_float = matches!(s, Value::Float(_)) || matches!(e, Value::Float(_)) || step_is_float;
    if is_float {
        let (a, b) = (ops::to_float(&s), ops::to_float(&e));
        let step = step_f.abs();
        if (b - a).abs() < step && (b - a).abs() > 0.0 {
            return Err(value_error(
                "range(): Argument #3 ($step) must not exceed the specified range",
            ));
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = ((b - a).abs() / step + 1e-9).floor() as usize;
        let mut out = Vec::with_capacity(n + 1);
        for i in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let k = i as f64;
            out.push(Value::Float(if a <= b {
                a + k * step
            } else {
                a - k * step
            }));
        }
        return Ok(Value::Array(PhpArray::list(out)));
    }
    let (a, b) = (ops::to_int(&s), ops::to_int(&e));
    let step = ops::to_int(step).unsigned_abs();
    if step == 0 {
        return Err(value_error("range(): Argument #3 ($step) cannot be 0"));
    }
    if a.abs_diff(b) < step && a != b {
        return Err(value_error(
            "range(): Argument #3 ($step) must not exceed the specified range",
        ));
    }
    let mut out = Vec::new();
    let mut c = a;
    loop {
        out.push(Value::Int(c));
        let step_i = i64::try_from(step).unwrap_or(i64::MAX);
        let next = if a <= b {
            c.checked_add(step_i)
        } else {
            c.checked_sub(step_i)
        };
        match next {
            Some(n) if (a <= b && n <= b) || (a > b && n >= b) => c = n,
            _ => break,
        }
    }
    Ok(Value::Array(PhpArray::list(out)))
}

/// The `range` function and the `..` operator.
pub fn range(_: &Call<'_>, args: Vec<Value>) -> R {
    range_values(
        &arg(&args, 0),
        &arg(&args, 1),
        &arg_or(&args, 2, Value::Int(1)),
    )
}

/// The `cycle` function.
pub fn cycle(_: &Call<'_>, args: Vec<Value>) -> R {
    let values = arg(&args, 0);
    let position = ops::to_int(&arg(&args, 1));
    let list: Vec<Value> = match &values {
        Value::Array(a) => a.values().cloned().collect(),
        Value::Object(o) if o.instance_of("ArrayAccess") => {
            if o.php_count().is_none() {
                symfony_deprecation_contracts::trigger_deprecation(
                    b"twig/twig",
                    b"3.12",
                    b"Passing a non-countable sequence of values to \"Twig\\Extension\\CoreExtension::cycle()\" is deprecated.",
                );
            }
            traverse(&values)?.into_iter().map(|(_, v)| v).collect()
        }
        _ => {
            return Err(runtime(
                "The \"cycle\" function expects an array or \"ArrayAccess\" as first argument.",
            ));
        }
    };
    if list.is_empty() {
        return Err(runtime(
            "The \"cycle\" function expects a non-empty sequence.",
        ));
    }
    let count = count_i64(list.len());
    let index = position % count;
    if let Value::Array(a) = &values {
        return Ok(a.get(&Key::Int(index)).cloned().unwrap_or(Value::Null));
    }
    Ok(usize::try_from(index)
        .ok()
        .and_then(|i| list.get(i).cloned())
        .unwrap_or(Value::Null))
}

/// The `random` function.
pub fn random(_: &Call<'_>, args: Vec<Value>) -> R {
    let values = arg(&args, 0);
    let max = arg(&args, 1);
    let rand_between = |min: i64, max: i64| -> R {
        if max < min {
            return Err(value_error(
                "mt_rand(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)",
            ));
        }
        let span = u64::try_from(i128::from(max) - i128::from(min) + 1).unwrap_or(u64::MAX);
        let offset = random_index(usize::try_from(span).unwrap_or(usize::MAX));
        Ok(Value::Int(
            min.saturating_add(i64::try_from(offset).unwrap_or(0)),
        ))
    };
    match &values {
        Value::Null => {
            if max.is_null() {
                rand_between(0, i64::from(i32::MAX))
            } else {
                rand_between(0, ops::to_int(&max))
            }
        }
        Value::Int(_) | Value::Float(_) => {
            let v = ops::to_int(&values);
            if max.is_null() {
                if v < 0 {
                    rand_between(v, 0)
                } else {
                    rand_between(0, v)
                }
            } else {
                rand_between(v, ops::to_int(&max))
            }
        }
        Value::String(s) => {
            if s.is_empty() {
                return Ok(Value::from(""));
            }
            let chars = mb::split_into_characters(s.as_bytes(), "random")?;
            let i = random_index(chars.len());
            Ok(Value::String(PhpString::from(chars[i].clone())))
        }
        other if !is_iterable_value(other) => Ok(other.clone()),
        other => {
            let array = array_of(other, true)?;
            if array.is_empty() {
                return Err(runtime(
                    "The \"random\" function cannot pick from an empty sequence or mapping.",
                ));
            }
            let i = random_index(array.len());
            Ok(array.values().nth(i).cloned().unwrap_or(Value::Null))
        }
    }
}

/// The constants the `constant` function and test can read: PHP's
/// predefined constants that templates use.
#[must_use]
pub fn php_constant(env: &Environment, name: &[u8]) -> Option<Value> {
    if let Some(value) = builtin_constant(name) {
        return Some(value);
    }
    let name = name.strip_prefix(b"\\").unwrap_or(name);
    let options = env.core_options();
    options
        .constants
        .iter()
        .find(|(n, _)| n.as_slice() == name)
        .map(|(_, v)| v.clone())
        .or_else(|| {
            options
                .enums
                .constant(&String::from_utf8_lossy(name))
                .cloned()
        })
}

/// `Class::CASE` for an enum case the compiler checked.
pub fn enum_constant(env: &Environment, name: &[u8]) -> R {
    php_constant(env, name).ok_or_else(|| {
        Error::exception(
            "Error",
            format!("Undefined constant {}", text(name)).into_bytes(),
        )
    })
}

/// `Class::cases()` for an enum the compiler checked.
pub fn enum_cases_of(env: &Environment, class: &[u8]) -> R {
    let options = env.core_options();
    let cases = options.enums.cases(&text(class)).unwrap_or_default();
    Ok(Value::Array(PhpArray::list(
        cases.iter().cloned().map(Value::Object),
    )))
}

/// The `enum_cases` function (`enumCases($enum)`).
pub fn enum_cases(call: &Call<'_>, args: Vec<Value>) -> R {
    let class = text(&string_of(&arg(&args, 0))?);
    if !call.env.core_options().enums.enum_exists(&class) {
        return Err(runtime(format!("Enum \"{class}\" does not exist.")));
    }
    enum_cases_of(call.env, class.as_bytes())
}

/// The `enum` function (`enum($enum)`): the enum's first case.
pub fn enum_first(call: &Call<'_>, args: Vec<Value>) -> R {
    let class = text(&string_of(&arg(&args, 0))?);
    let options = call.env.core_options();
    let Some(cases) = options.enums.cases(&class) else {
        return Err(runtime(format!("\"{class}\" is not an enum.")));
    };
    match cases.first() {
        Some(case) => Ok(Value::Object(case.clone())),
        None => Err(runtime(format!("\"{class}\" is an empty enum."))),
    }
}

/// The PHP constants the port knows.
#[allow(clippy::match_same_arms)] // a table of names
fn builtin_constant(name: &[u8]) -> Option<Value> {
    Some(match name {
        b"PHP_EOL" => Value::from("\n"),
        b"PHP_INT_MAX" => Value::Int(i64::MAX),
        b"PHP_INT_MIN" => Value::Int(i64::MIN),
        b"PHP_INT_SIZE" => Value::Int(8),
        b"PHP_VERSION_ID" => Value::Int(80427),
        b"M_PI" => Value::Float(std::f64::consts::PI),
        b"E_ALL" => Value::Int(32767),
        b"E_ERROR" => Value::Int(1),
        b"E_WARNING" => Value::Int(2),
        b"E_NOTICE" => Value::Int(8),
        b"E_DEPRECATED" => Value::Int(8192),
        b"E_USER_DEPRECATED" => Value::Int(16384),
        b"DATE_W3C" | b"DATE_ATOM" | b"DATE_RFC3339" => Value::from("Y-m-d\\TH:i:sP"),
        b"ENT_QUOTES" => Value::Int(3),
        b"JSON_PRETTY_PRINT" => Value::Int(128),
        b"JSON_UNESCAPED_SLASHES" => Value::Int(64),
        b"JSON_UNESCAPED_UNICODE" => Value::Int(256),
        b"JSON_THROW_ON_ERROR" => Value::Int(4_194_304),
        b"ArrayObject::STD_PROP_LIST" | b"JSON_HEX_TAG" => Value::Int(1),
        b"ArrayObject::ARRAY_AS_PROPS" => Value::Int(2),
        _ => return None,
    })
}

/// `constant($constant, $object, $checkDefined)`.
pub fn constant_value(
    env: &Environment,
    constant: &Value,
    object: &Value,
    check_defined: bool,
) -> R {
    let mut name = string_of(constant)?;
    if let Value::Object(o) = object {
        if name == b"class" {
            return Ok(if check_defined {
                Value::Bool(true)
            } else {
                Value::from(o.class())
            });
        }
        let mut full = o.class().as_bytes().to_vec();
        full.extend_from_slice(b"::");
        full.extend_from_slice(&name);
        name = full;
    }
    match php_constant(env, &name) {
        Some(v) => Ok(if check_defined { Value::Bool(true) } else { v }),
        None if check_defined => Ok(Value::Bool(false)),
        None => {
            if name.len() >= 7 && name[name.len() - 7..].eq_ignore_ascii_case(b"::class") {
                return Err(runtime(format!(
                    "You cannot use the Twig function \"constant\" to access \"{}\". You could provide an object and call constant(\"class\", $object) or use the class name directly as a string.",
                    text(&name)
                )));
            }
            Err(runtime(format!(
                "Constant \"{}\" is undefined.",
                text(&name)
            )))
        }
    }
}

/// The `constant` function.
pub fn constant(call: &Call<'_>, args: Vec<Value>) -> R {
    constant_value(
        call.env,
        &arg(&args, 0),
        &arg(&args, 1),
        arg(&args, 2).is_truthy(),
    )
}

/// `include($env, $context, $template, $variables, $withContext,
/// $ignoreMissing)`.
pub fn include_template(
    env: &Environment,
    context: &PhpArray,
    template: &Value,
    variables: &Value,
    with_context: bool,
    ignore_missing: bool,
) -> R {
    let mut variables = match variables {
        Value::Array(a) => a.clone(),
        other => {
            return Err(type_error(format!(
                "Twig\\Extension\\CoreExtension::include(): Argument #4 ($variables) must be of type array, {} given",
                other.debug_type()
            )));
        }
    };
    if with_context {
        variables = merge_arrays(&[Value::Array(context.clone()), Value::Array(variables)])?;
    }
    let names = match template {
        Value::Array(a) => a.clone(),
        other => PhpArray::list([other.clone()]),
    };
    let loaded = match env.resolve_template(&names) {
        Ok(t) => t,
        Err(e) if e.kind() == ErrorKind::Loader && ignore_missing => return Ok(Value::from("")),
        Err(e) => return Err(e),
    };
    let rendered = loaded.render(env, variables)?;
    if rendered.is_empty() {
        return Ok(Value::from(""));
    }
    Ok(Markup::value(rendered, env.charset_string()))
}

/// The `include` function.
pub fn include(call: &Call<'_>, args: Vec<Value>) -> R {
    if args.len() >= 5 {
        let message: &[u8] = if args[4].is_truthy() {
            b"The \"sandboxed\" argument of the \"include\" function is deprecated, use the \"render_sandboxed\" function to render untrusted templates instead."
        } else {
            b"The \"sandboxed\" argument of the \"include\" function is deprecated, remove the argument as \"false\" has no effect."
        };
        symfony_deprecation_contracts::trigger_deprecation(b"twig/twig", b"3.29", message);
    }
    include_template(
        call.env,
        call.context,
        &arg(&args, 0),
        &arg_or(&args, 1, Value::Array(PhpArray::new())),
        arg_or(&args, 2, Value::Bool(true)).is_truthy(),
        arg(&args, 3).is_truthy(),
    )
}

/// The `include_only` function.
pub fn include_only(call: &Call<'_>, args: Vec<Value>) -> R {
    include_template(
        call.env,
        &PhpArray::new(),
        &arg(&args, 0),
        &arg_or(&args, 1, Value::Array(PhpArray::new())),
        false,
        arg(&args, 2).is_truthy(),
    )
}

/// The `source` function.
pub fn source(call: &Call<'_>, args: Vec<Value>) -> R {
    let name = string_of(&arg(&args, 0))?;
    let ignore_missing = arg(&args, 1).is_truthy();
    let loader = call.env.loader();
    let result = match loader {
        Some(l) => l.source_context(&name),
        None => Err(Error::loader(
            format!("Template \"{}\" is not defined.", text(&name)),
            -1,
            None,
        )),
    };
    match result {
        Ok(source) => Ok(Value::String(PhpString::from(source.code()))),
        Err(e) if e.kind() == ErrorKind::Loader && ignore_missing => Ok(Value::from("")),
        Err(e) => Err(e),
    }
}

/// The `escape` filter (`EscaperRuntime::escape()`).
pub fn escape(call: &Call<'_>, args: Vec<Value>) -> R {
    let strategy = string_of(&arg_or(&args, 1, Value::from("html")))?;
    let charset = arg(&args, 2);
    let charset = if charset.is_null() {
        None
    } else {
        Some(string_of(&charset)?)
    };
    let autoescape = arg(&args, 3).is_truthy();
    call.env
        .escaper()
        .escape(&arg(&args, 0), &strategy, charset.as_deref(), autoescape)
}

// ---------------------------------------------------------------------
// getAttribute

fn class_methods(object: &dyn PhpObject) -> Vec<(String, String)> {
    let mut methods = object.methods();
    if object.class() == "Closure" {
        methods.push("__invoke".to_owned());
    }
    methods.sort();
    let lc: Vec<String> = methods.iter().map(|m| m.to_ascii_lowercase()).collect();
    let mut cache: Vec<(String, String)> = Vec::new();
    let set = |cache: &mut Vec<(String, String)>, key: String, method: &str, overwrite: bool| {
        if let Some(slot) = cache.iter_mut().find(|(k, _)| *k == key) {
            if overwrite {
                method.clone_into(&mut slot.1);
            }
        } else {
            cache.push((key, method.to_owned()));
        }
    };
    for (i, method) in methods.iter().enumerate() {
        set(&mut cache, method.clone(), method, true);
        let lc_name = &lc[i];
        set(&mut cache, lc_name.clone(), method, true);
        let prefix = if lc_name.starts_with("get") {
            3
        } else if lc_name.starts_with("is") {
            2
        } else if let Some(rest) = lc_name.strip_prefix("has") {
            if lc.contains(&format!("is{rest}")) {
                continue;
            }
            3
        } else {
            continue;
        };
        let short = &lc_name[prefix..];
        if !short.is_empty() {
            let name = format!("{}{}", &short[..1], method.get(prefix + 1..).unwrap_or(""));
            set(&mut cache, name, method, false);
            set(&mut cache, short.to_owned(), method, false);
        }
    }
    cache
}

/// `getAttribute($env, $source, $object, $item, $arguments, $type,
/// $isDefinedTest, $ignoreStrictCheck, $sandboxed, $lineno)` (the sandbox
/// is not ported: `$sandboxed` is false).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn get_attribute(
    env: &Environment,
    source: &Arc<Source>,
    object: &Value,
    item: &Value,
    arguments: &PhpArray,
    kind: &str,
    is_defined_test: bool,
    ignore_strict_check: bool,
    lineno: i64,
) -> R {
    let error = |message: String| Error::runtime(message, lineno, Some(Arc::clone(source)));
    let lenient = ignore_strict_check || !env.is_strict_variables();

    if kind != METHOD_CALL {
        let mut array_item = match item {
            Value::Bool(_) | Value::Float(_) => Value::Int(ops::to_int(item)),
            other => other.clone(),
        };
        if let (Value::Object(i), Value::Object(o)) = (&array_item, object)
            && i.is_stringable()
            && (o.instance_of("ArrayObject")
                || o.instance_of("ArrayIterator")
                || matches!(o.class(), "ArrayObject" | "ArrayIterator"))
        {
            array_item = Value::String(super::to_php_string(&array_item)?);
        }
        let exists = match object {
            Value::Array(a) => {
                array_item = Value::String(super::to_php_string(&array_item)?);
                let key = Key::from_value(&array_item).unwrap_or(Key::Int(0));
                a.contains_key(&key)
            }
            Value::Object(o) => match o.offset_exists(&array_item) {
                Some(r) => r?,
                None => false,
            },
            _ => false,
        };
        if exists {
            if is_defined_test {
                return Ok(Value::Bool(true));
            }
            return match object {
                Value::Array(a) => {
                    let key = Key::from_value(&array_item).unwrap_or(Key::Int(0));
                    Ok(a.get(&key).cloned().unwrap_or(Value::Null))
                }
                Value::Object(o) => match o.offset_get(&array_item) {
                    Some(r) => Ok(r?),
                    None => Ok(Value::Null),
                },
                _ => Ok(Value::Null),
            };
        }
        let is_object = matches!(object, Value::Object(_));
        if kind == ARRAY_CALL || !is_object {
            if is_defined_test {
                return Ok(Value::Bool(false));
            }
            if lenient {
                return Ok(Value::Null);
            }
            let item_text = |v: &Value| -> R<String> { Ok(text(&string_of(v)?)) };
            let message = match object {
                Value::Object(o) if o.instance_of("ArrayAccess") => {
                    if matches!(array_item, Value::Object(_) | Value::Array(_)) {
                        format!(
                            "Key of type \"{}\" does not exist in ArrayAccess-able object of class \"{}\".",
                            array_item.debug_type(),
                            object.debug_type()
                        )
                    } else {
                        format!(
                            "Key \"{}\" does not exist in ArrayAccess-able object of class \"{}\".",
                            item_text(&array_item)?,
                            object.debug_type()
                        )
                    }
                }
                Value::Object(_) => format!(
                    "Impossible to access a key \"{}\" on an object of class \"{}\" that does not implement ArrayAccess interface.",
                    item_text(item)?,
                    object.debug_type()
                ),
                Value::Array(a) => {
                    if a.is_empty() {
                        format!(
                            "Key \"{}\" does not exist as the sequence/mapping is empty.",
                            item_text(&array_item)?
                        )
                    } else {
                        let keys: Vec<String> = a.keys().map(|k| text(&k.bytes())).collect();
                        format!(
                            "Key \"{}\" for sequence/mapping with keys \"{}\" does not exist.",
                            item_text(&array_item)?,
                            keys.join(", ")
                        )
                    }
                }
                Value::Null if kind == ARRAY_CALL => format!(
                    "Impossible to access a key (\"{}\") on a null variable.",
                    item_text(item)?
                ),
                other if kind == ARRAY_CALL => format!(
                    "Impossible to access a key (\"{}\") on a {} variable (\"{}\").",
                    item_text(item)?,
                    other.debug_type(),
                    item_text(other)?
                ),
                Value::Null => format!(
                    "Impossible to access an attribute (\"{}\") on a null variable.",
                    item_text(item)?
                ),
                other => format!(
                    "Impossible to access an attribute (\"{}\") on a {} variable (\"{}\").",
                    item_text(item)?,
                    other.debug_type(),
                    item_text(other)?
                ),
            };
            return Err(error(message));
        }
    }

    let item = string_of(item)?;
    let item_str = text(&item);

    let Value::Object(o) = object else {
        if is_defined_test {
            return Ok(Value::Bool(false));
        }
        if lenient {
            return Ok(Value::Null);
        }
        let message = match object {
            Value::Null => {
                format!("Impossible to invoke a method (\"{item_str}\") on a null variable.")
            }
            Value::Array(_) => {
                format!("Impossible to invoke a method (\"{item_str}\") on a sequence/mapping.")
            }
            other => format!(
                "Impossible to invoke a method (\"{item_str}\") on a {} variable (\"{}\").",
                other.debug_type(),
                text(&string_of(other)?)
            ),
        };
        return Err(error(message));
    };

    if kind != METHOD_CALL {
        if let Some(value) = o.property(&item) {
            if is_defined_test {
                return Ok(Value::Bool(true));
            }
            return Ok(value);
        }
        if o.properties().contains_key(&Key::from_bytes(&item)) {
            if is_defined_test {
                return Ok(Value::Bool(true));
            }
            return Ok(Value::Null);
        }
        let mut constant = o.class().as_bytes().to_vec();
        constant.extend_from_slice(b"::");
        constant.extend_from_slice(&item);
        if let Some(value) = php_constant(env, &constant) {
            if is_defined_test {
                return Ok(Value::Bool(true));
            }
            return Ok(value);
        }
    }

    let cache = class_methods(o.as_ref());
    let lookup = |key: &str| cache.iter().find(|(k, _)| k == key).map(|(_, m)| m.clone());
    let (method, is_call) = if let Some(m) = lookup(&item_str) {
        (m, false)
    } else if let Some(m) = lookup(&item_str.to_ascii_lowercase()) {
        (m, false)
    } else if lookup("__call").is_some() {
        (item_str.clone(), true)
    } else {
        if is_defined_test {
            return Ok(Value::Bool(false));
        }
        if lenient {
            return Ok(Value::Null);
        }
        return Err(error(format!(
            "Neither the property \"{item_str}\" nor one of the methods \"{item_str}()\", \"get{item_str}()\", \"is{item_str}()\", \"has{item_str}()\" or \"__call()\" exist and have public access in class \"{}\".",
            o.class()
        )));
    };
    if is_defined_test {
        return Ok(Value::Bool(true));
    }
    // `$object->$method(...$arguments)`: named arguments bind by the
    // method's parameters when the object gives them
    let named = arguments.iter().any(|(k, _)| matches!(k, Key::Str(_)));
    let arguments: Vec<Value> = match o.method_parameters(&method) {
        Some(parameters) if named => {
            php_value::args::bind(&format!("{}::{method}", o.class()), &parameters, arguments)?
        }
        _ => arguments.values().cloned().collect(),
    };
    match o.call_method(&method, &arguments) {
        Some(Ok(value)) => Ok(value),
        Some(Err(thrown)) => {
            if is_call && lenient && thrown_is(&thrown, "BadMethodCallException") {
                return Ok(Value::Null);
            }
            Err(thrown.into())
        }
        None => Ok(Value::Null),
    }
}

fn thrown_is(thrown: &Thrown, class: &str) -> bool {
    thrown.class.trim_start_matches('\\') == class
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_half_away_from_zero() {
        assert!((php_round(0.285, 2) - 0.29).abs() < 1e-12);
        assert!((php_round(-1.5, 0) + 2.0).abs() < 1e-12);
        assert!((php_round(1234.5678, -2) - 1200.0).abs() < 1e-9);
        assert!((php_round(2.5, 0) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn number_format_groups() {
        assert_eq!(
            number_format(1234.5678, 2, b",", b" "),
            b"1 234,57".to_vec()
        );
        assert_eq!(number_format(0.0, 0, b".", b","), b"0".to_vec());
    }

    #[test]
    fn sprintf_specifiers() {
        let s = php_sprintf(
            b"%s-%05d-%.2f-%x",
            &[
                Value::from("a"),
                Value::Int(42),
                Value::Float(1.005),
                Value::Int(255),
            ],
        )
        .unwrap();
        assert_eq!(s, b"a-00042-1.01-ff".to_vec());
    }
}
