import { test, expect, Page } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  await page.goto('http://localhost:8080');
});

const TODO_ITEMS = [
  'buy some cheese',
  'feed the cat',
  'book a doctors appointment'
];

test.describe('New Todo', () => {
  test('should allow me to add todo items', async ({ page }) => {
    // // Create 1st todo.
    // await page.locator('.new-todo').fill(TODO_ITEMS[0]);
    // await page.locator('.new-todo').press('Enter');

    // // Make sure the list only has one todo item.
    // await expect(page.locator('.view label')).toHaveText([
    //   TODO_ITEMS[0]
    // ]);

    // // Create 2nd todo.
    // await page.locator('.new-todo').fill(TODO_ITEMS[1]);
    // await page.locator('.new-todo').press('Enter');

    // // Make sure the list now has two todo items.
    // await expect(page.locator('.view label')).toHaveText([
    //   TODO_ITEMS[0],
    //   TODO_ITEMS[1]
    // ]);

    // await checkNumberOfTodosInLocalStorage(page, 2);
  });

  // test('should clear text input field when an item is added', async ({ page }) => {
  //   // Create one todo item.
  //   await page.locator('.new-todo').fill(TODO_ITEMS[0]);
  //   await page.locator('.new-todo').press('Enter');

  //   // Check that input is empty.
  //   await expect(page.locator('.new-todo')).toBeEmpty();
  //   await checkNumberOfTodosInLocalStorage(page, 1);
  // });

  // test('should append new items to the bottom of the list', async ({ page }) => {
  //   // Create 3 items.
  //   await createDefaultTodos(page);

  //   // Check test using different methods.
  //   await expect(page.locator('.todo-count')).toHaveText('3 items left');
  //   await expect(page.locator('.todo-count')).toContainText('3');
  //   await expect(page.locator('.todo-count')).toHaveText(/3/);

  //   // Check all items in one call.
  //   await expect(page.locator('.view label')).toHaveText(TODO_ITEMS);
  //   await checkNumberOfTodosInLocalStorage(page, 3);
  // });

  // test('should show #main and #footer when items added', async ({ page }) => {
  //   await page.locator('.new-todo').fill(TODO_ITEMS[0]);
  //   await page.locator('.new-todo').press('Enter');

  //   await expect(page.locator('.main')).toBeVisible();
  //   await expect(page.locator('.footer')).toBeVisible();
  //   await checkNumberOfTodosInLocalStorage(page, 1);
  // });
});

// test.describe('Routing', () => {
//   // test.beforeEach(async ({ page }) => {
//   //   await createDefaultTodos(page);
//   //   // make sure the app had a chance to save updated todos in storage
//   //   // before navigating to a new view, otherwise the items can get lost :(
//   //   // in some frameworks like Durandal
//   //   await checkTodosInLocalStorage(page, TODO_ITEMS[0]);
//   // });

//   test('should allow me to display active items', async ({ page }) => {
//     // await page.locator('.todo-list li .toggle').nth(1).check();
//     // await checkNumberOfCompletedTodosInLocalStorage(page, 1);
//     // await page.locator('.filters >> text=Active').click();
//     // await expect(page.locator('.todo-list li')).toHaveCount(2);
//     // await expect(page.locator('.todo-list li')).toHaveText([TODO_ITEMS[0], TODO_ITEMS[2]]);
//   });

//   // test('should respect the back button', async ({ page }) => {
//   //   await page.locator('.todo-list li .toggle').nth(1).check();
//   //   await checkNumberOfCompletedTodosInLocalStorage(page, 1);

//   //   await test.step('Showing all items', async () => {
//   //     await page.locator('.filters >> text=All').click();
//   //     await expect(page.locator('.todo-list li')).toHaveCount(3);
//   //   });

//   //   await test.step('Showing active items', async () => {
//   //     await page.locator('.filters >> text=Active').click();
//   //   });

//   //   await test.step('Showing completed items', async () => {
//   //     await page.locator('.filters >> text=Completed').click();
//   //   });

//   //   await expect(page.locator('.todo-list li')).toHaveCount(1);
//   //   await page.goBack();
//   //   await expect(page.locator('.todo-list li')).toHaveCount(2);
//   //   await page.goBack();
//   //   await expect(page.locator('.todo-list li')).toHaveCount(3);
//   // });

//   // test('should allow me to display completed items', async ({ page }) => {
//   //   await page.locator('.todo-list li .toggle').nth(1).check();
//   //   await checkNumberOfCompletedTodosInLocalStorage(page, 1);
//   //   await page.locator('.filters >> text=Completed').click();
//   //   await expect(page.locator('.todo-list li')).toHaveCount(1);
//   // });

//   // test('should allow me to display all items', async ({ page }) => {
//   //   await page.locator('.todo-list li .toggle').nth(1).check();
//   //   await checkNumberOfCompletedTodosInLocalStorage(page, 1);
//   //   await page.locator('.filters >> text=Active').click();
//   //   await page.locator('.filters >> text=Completed').click();
//   //   await page.locator('.filters >> text=All').click();
//   //   await expect(page.locator('.todo-list li')).toHaveCount(3);
//   // });

//   // test('should highlight the currently applied filter', async ({ page }) => {
//   //   await expect(page.locator('.filters >> text=All')).toHaveClass('selected');
//   //   await page.locator('.filters >> text=Active').click();
//   //   // Page change - active items.
//   //   await expect(page.locator('.filters >> text=Active')).toHaveClass('selected');
//   //   await page.locator('.filters >> text=Completed').click();
//   //   // Page change - completed items.
//   //   await expect(page.locator('.filters >> text=Completed')).toHaveClass('selected');
//   // });
// });

// // async function createDefaultTodos(page: Page) {
// //   for (const item of TODO_ITEMS) {
// //     await page.locator('.new-todo').fill(item);
// //     await page.locator('.new-todo').press('Enter');
// //   }
// // }

// // async function checkNumberOfTodosInLocalStorage(page: Page, expected: number) {
// //   return await page.waitForFunction(e => {
// //     return JSON.parse(localStorage['react-todos']).length === e;
// //   }, expected);
// // }

// // async function checkNumberOfCompletedTodosInLocalStorage(page: Page, expected: number) {
// //   return await page.waitForFunction(e => {
// //     return JSON.parse(localStorage['react-todos']).filter(i => i.completed).length === e;
// //   }, expected);
// // }

// // async function checkTodosInLocalStorage(page: Page, title: string) {
// //   return await page.waitForFunction(t => {
// //     return JSON.parse(localStorage['react-todos']).map(i => i.title).includes(t);
// //   }, title);
// // }
