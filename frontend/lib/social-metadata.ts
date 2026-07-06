export const SOCIAL_IMAGE_VERSION = "20260314";
export const HOME_SOCIAL_IMAGE_VERSION = "20260315-home1";

const socialImageAlt = "Cerul — where video becomes citable";

function createOpenGraphImages(version: string) {
  return [
    {
      url: `/og-image.png?v=${version}`,
      width: 1200,
      height: 630,
      alt: socialImageAlt,
    },
  ];
}

function createTwitterImages(version: string) {
  return [
    {
      url: `/og-twitter.png?v=${version}`,
      width: 800,
      height: 418,
      alt: socialImageAlt,
    },
  ];
}

export const defaultOpenGraphImages = createOpenGraphImages(SOCIAL_IMAGE_VERSION);
export const defaultTwitterImages = createTwitterImages(SOCIAL_IMAGE_VERSION);
export const homeOpenGraphImages = createOpenGraphImages(HOME_SOCIAL_IMAGE_VERSION);
export const homeTwitterImages = createTwitterImages(HOME_SOCIAL_IMAGE_VERSION);
