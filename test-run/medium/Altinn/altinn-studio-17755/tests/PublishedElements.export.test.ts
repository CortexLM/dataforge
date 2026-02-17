import { PublishedElements } from '@studio/pure-functions';

const elementName = 'widgets';
const otherElement = 'gadgets';

const fileNames = [
  '_index.json',
  `${elementName}/1.json`,
  `${elementName}/2.json`,
  `${elementName}/10.json`,
  `${elementName}/_index.json`,
  `${elementName}/_latest.json`,
  `${elementName}/README.md`,
  `${otherElement}/1.json`,
  `${otherElement}/_latest.json`,
  `${otherElement}/_index.json`,
  `${otherElement}/2.JSON`,
];

describe('PublishedElements (from pure-functions export)', () => {
  it('detects published elements and latest versions with mixed file types', () => {
    const publishedElements = new PublishedElements(fileNames);

    expect(publishedElements.isPublished(elementName)).toBe(true);
    expect(publishedElements.latestVersionOrNull(elementName)).toBe(10);

    expect(publishedElements.isPublished(otherElement)).toBe(true);
    expect(publishedElements.latestVersionOrNull(otherElement)).toBe(2);
  });

  it('returns null and false when required versions are missing', () => {
    const publishedElements = new PublishedElements([
      '_index.json',
      'widgets/1.json',
      'widgets/_index.json',
    ]);

    expect(publishedElements.isPublished('widgets')).toBe(false);
    expect(publishedElements.latestVersionOrNull('widgets')).toBeNull();
  });
});
