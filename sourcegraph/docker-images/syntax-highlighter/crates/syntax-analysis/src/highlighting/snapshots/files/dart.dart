// Libraries and Imports
library some.lib.here;

import 'dart:math';

part of 'other.dart';

// Classes and Objects
sealed class Person {
  String name;
  int age;

  Person(this.name, this.age);

  void sayHello() {
    print("Hello, my name is $name and I'm $age years old.");
  }
}


// Enums
enum Status { active, inactive, suspended }

void main() {
  // Variables and Data Types
  int age = 30;
  double pi = 3.14;
  String name = "John Doe";
  bool isStudent = true;

  if (age == null) return;

  // Conditional Statements
  if (age > 18) {
    print("You are an adult");
  } else {
    print("You are a minor");
  }

  // Loops
  for (int i = 0; i < 5; i++) {
    print("Count: $i");
  }

  List<String> fruits = ['apple', 'banana', 'cherry'];
  for (String fruit in fruits) {
    print("Fruit: $fruit");
  }

  // Functions
  int add(int a, int b) {
    return a + b;
  }

  print("Sum: ${add(5, 3)}");

  final person = Person("Alice", 25);
  person.sayHello();

  // Lists and Maps
  var numbers = <int>[1, 2, 3, 4, 5];
  numbers.add(6);

  Map<String, String> capitals = {'USA': 'Washington, D.C.', 'France': 'Paris'};
  capitals['Germany'] = 'Berlin';

  // Exception Handling
  try {
    int result = 12 ~/ 0;
    print("Result: $result");
  } catch (e) {
    print("Error: $e");
  }

  Status userStatus = Status.active;
  print("User Status: $userStatus");

  int random = Random().nextInt(100);
  print("Random Number: $random");

  fetchData().then((value) => print(value));

  printData();
}

(String, int) userInfo(Map<String, dynamic> json) {
  return (json['name'] as String, json['height'] as int);
}

// Async Programming (Future)
Future<String> fetchData() {
  return Future.delayed(Duration(seconds: 2), () => "Data loaded");
}

// Async Programming (Async/Await)
Future<void> printData() async {
  String data = await fetchData();
  print(data);
}

void keywords() {
  const foobar = 'test';
  assert(1 == 1);
  switch (1) {
    case 1:
      print('1');
      break;
  }

  try {
    throw Exception();
  } catch(e) {
    rethrow;
  }
}

void lambdaparam() {
    Directory("/").map((file) => file.path);
}
