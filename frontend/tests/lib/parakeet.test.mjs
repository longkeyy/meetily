import { describe, expect, test } from "bun:test";
import {
  PARAKEET_CTC_ZH_CN_MODEL,
  getModelDisplayName,
  getModelDownloadProgress,
  getParakeetModelSizeMb,
  getTranscriptionLanguageCapability,
  supportsTranscriptionLanguage,
} from '../../src/lib/parakeet';

describe('Parakeet model metadata', () => {
  test('reads the Rust Downloading struct variant', () => {
    expect(getModelDownloadProgress({ Downloading: { progress: 42 } })).toBe(42);
    expect(getModelDownloadProgress('Available')).toBeNull();
  });

  test('describes the Mandarin CoreML model', () => {
    expect(getModelDisplayName(PARAKEET_CTC_ZH_CN_MODEL)).toBe('Mandarin CoreML');
    expect(getParakeetModelSizeMb(PARAKEET_CTC_ZH_CN_MODEL)).toBe(582);

    const capability = getTranscriptionLanguageCapability(
      'parakeet',
      PARAKEET_CTC_ZH_CN_MODEL
    );
    expect(capability.allowsLanguageSelection).toBe(false);
    expect(capability.displayName).toBe('Mandarin Chinese + English');
    expect(capability.analyticsLanguage).toBe('zh-en');
    expect(capability.description).toContain('Mandarin Chinese');
    expect(capability.description).toContain('Translation is not available');
  });

  test('keeps English TDT and selectable-language providers distinct', () => {
    expect(
      getTranscriptionLanguageCapability('parakeet', 'parakeet-tdt-0.6b-v3-int8')
        .analyticsLanguage
    ).toBe('en');
    expect(getTranscriptionLanguageCapability('localWhisper', 'large-v3'))
      .toMatchObject({ allowsLanguageSelection: true });
  });

  test('allows supported Qwen3-ASR language hints without translation mode', () => {
    const capability = getTranscriptionLanguageCapability(
      'qwen3Asr',
      'qwen3-asr-0.6b-int8'
    );
    expect(capability.allowsLanguageSelection).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'zh')).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'yue')).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'auto')).toBe(true);
    expect(supportsTranscriptionLanguage(capability, 'auto-translate')).toBe(false);
    expect(supportsTranscriptionLanguage(capability, 'uk')).toBe(false);
    expect(capability.description).toContain('Choose Chinese');
  });

  test('describes SenseVoice as automatic detection for its five supported languages', () => {
    const capability = getTranscriptionLanguageCapability(
      'senseVoice',
      'sense-voice-small-int8'
    );
    expect(capability.allowsLanguageSelection).toBe(false);
    expect(capability.analyticsLanguage).toBe('auto');
    expect(capability.displayName).toContain('Mandarin');
    expect(capability.description).toContain('Cantonese');
    expect(capability.description).toContain('Korean');
  });
});
